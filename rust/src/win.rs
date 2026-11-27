//! 生 Win32 GUI 本体。Delphi実験版 `Mp3SilenceMini.pas` の Rust 移植。
//!
//! - ウィンドウ構成・文言・レイアウト計算は Delphi 版と同一。
//! - ffmpeg 探索順: ① exe と同じフォルダ ② %LocalAppData%\Mp3SilenceCutter\bin
//!   ③ PATH（SearchPathW）。
//! - 実行: CreateProcessW（CREATE_NO_WINDOW）、stdout+stderr を1本のパイプで
//!   受け取り、ポーリングしながらメッセージポンプを回す（Delphi 版と同じ）。
//! - 起動時は実行ファイルパスを lpApplicationName とコマンドライン先頭の
//!   両方に渡す（Delphi 版・第2回修正で確立した堅牢な形）。
//! - stdin は NUL デバイス。開けなければパイプにフォールバックし、
//!   無効なハンドルは絶対に渡さない。

use std::mem;
use std::ptr;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::{
    GetStockObject, UpdateWindow, COLOR_BTNFACE, DEFAULT_GUI_FONT, HBRUSH, HFONT,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::Diagnostics::Debug::FormatMessageW;
use windows_sys::Win32::System::Environment::GetEnvironmentVariableW;
use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
use windows_sys::Win32::System::Pipes::{CreatePipe, PeekNamedPipe};
use windows_sys::Win32::System::SystemServices::SS_CENTERIMAGE;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::parse::{fmt_sec, parse_silence_detect_log};

const IDC_PATH: u32 = 101;
const IDC_BROWSE: u32 = 102;
const IDC_DETECT: u32 = 103;
const IDC_STATUS: u32 = 104;
const IDC_LIST: u32 = 105;

const CLASS_NAME: &str = "Mp3SilenceMiniRustClass";
const APP_TITLE: &str = "MP3無音検出ミニ（Rust実験版）";

// GetExitCodeProcess が「実行中」を返す値（STILL_ACTIVE）。
const STILL_ACTIVE_U32: u32 = 259;
// FILE_ATTRIBUTE_DIRECTORY。
const FILE_ATTRIBUTE_DIRECTORY_U32: u32 = 0x10;
// MAKELANGID(LANG_NEUTRAL, SUBLANG_DEFAULT)。
const LANGID_NEUTRAL_DEFAULT: u32 = 0x0400;
// FORMAT_MESSAGE_ALLOCATE_BUFFER | FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS。
const FORMAT_MESSAGE_FLAGS: u32 = 0x0000_0100 | 0x0000_1000 | 0x0000_0200;

static mut H_MAIN: HWND = ptr::null_mut();
static mut H_EDIT: HWND = ptr::null_mut();
static mut H_BROWSE: HWND = ptr::null_mut();
static mut H_DETECT: HWND = ptr::null_mut();
static mut H_STATUS: HWND = ptr::null_mut();
static mut H_LIST: HWND = ptr::null_mut();

/* ---------- 小物ヘルパー ---------- */

/// 終端 NUL なしの UTF-16 列。
fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// 終端 NUL 付きの UTF-16 列（API へ渡す用）。
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 終端 NUL を保証したコピー。
fn zterm(v: &[u16]) -> Vec<u16> {
    let mut out = v.to_vec();
    if out.last() != Some(&0) {
        out.push(0);
    }
    out
}

fn loword(v: usize) -> u32 {
    (v & 0xFFFF) as u32
}

fn hiword(v: usize) -> u32 {
    ((v >> 16) & 0xFFFF) as u32
}

unsafe fn set_status(s: &str) {
    let w = wide(s);
    SetWindowTextW(H_STATUS, w.as_ptr());
}

unsafe fn message_box(body: &str, title: &str, flags: MESSAGEBOX_STYLE) {
    let b = wide(body);
    let t = wide(title);
    MessageBoxW(H_MAIN, b.as_ptr(), t.as_ptr(), flags);
}

/// FileExistsW 相当: 実在し、ディレクトリでないこと。
unsafe fn file_exists(path: &[u16]) -> bool {
    let p = zterm(path);
    let attr = GetFileAttributesW(p.as_ptr());
    attr != INVALID_FILE_ATTRIBUTES && (attr & FILE_ATTRIBUTE_DIRECTORY_U32) == 0
}

/* ---------- ffmpeg 探索（Delphi 版 FindFFmpeg の移植） ---------- */

/// 探索順: ① exe と同じフォルダ ② 本編の展開先 ③ PATH。
/// 見つからなければ None。探索自体が失敗しても panic しない。
unsafe fn find_ffmpeg() -> Option<Vec<u16>> {
    let mut buf = [0u16; 1024];

    // ① exe と同じフォルダ
    let n = GetModuleFileNameW(ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32) as usize;
    if n > 0 && n <= buf.len() {
        let full = &buf[..n];
        if let Some(pos) = full.iter().rposition(|&c| c == 0x5C) {
            // '\'
            let mut cand = full[..=pos].to_vec();
            cand.extend_from_slice(&utf16("ffmpeg.exe"));
            if file_exists(&cand) {
                return Some(cand);
            }
        }
    }

    // ② %LocalAppData%\Mp3SilenceCutter\bin\ffmpeg.exe
    let env_name = wide("LOCALAPPDATA");
    let n = GetEnvironmentVariableW(env_name.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) as usize;
    if n > 0 && n < buf.len() {
        let mut cand = buf[..n].to_vec();
        cand.extend_from_slice(&utf16("\\Mp3SilenceCutter\\bin\\ffmpeg.exe"));
        if file_exists(&cand) {
            return Some(cand);
        }
    }

    // ③ PATH（SearchPathW）。最終引数 lpFilePart は「PWSTR 変数へのポインタ」
    //    で、API がそこへ書き込む。必ず変数のアドレスを渡すこと
    //    （Delphi 版・第1回修正で学んだ落とし穴）。
    let fname = wide("ffmpeg.exe");
    let mut part: *mut u16 = ptr::null_mut();
    let n = SearchPathW(
        ptr::null(),
        fname.as_ptr(),
        ptr::null(),
        buf.len() as u32,
        buf.as_mut_ptr(),
        &mut part,
    ) as usize;
    if n > 0 && n < buf.len() {
        return Some(buf[..n].to_vec());
    }

    None
}

/* ---------- WinErrorText 相当 ---------- */

/// GetLastError コードのシステムメッセージ。末尾の句読点・改行は取り除く。
unsafe fn win_error_text(code: u32) -> String {
    let mut buf_ptr: *mut u16 = ptr::null_mut();
    let n = FormatMessageW(
        FORMAT_MESSAGE_FLAGS,
        ptr::null(),
        code,
        LANGID_NEUTRAL_DEFAULT,
        &mut buf_ptr as *mut *mut u16 as *mut u16,
        0,
        ptr::null(),
    );
    if n == 0 || buf_ptr.is_null() {
        return String::new();
    }
    let slice = std::slice::from_raw_parts(buf_ptr, n as usize);
    let mut s = String::from_utf16_lossy(slice);
    LocalFree(buf_ptr as HLOCAL);
    while let Some(c) = s.chars().last() {
        if c == '\r' || c == '\n' || c == ' ' || c == '.' || c == '。' {
            s.pop();
        } else {
            break;
        }
    }
    s
}

/* ---------- RunCapture 相当 ---------- */

/// ExePath を Args で実行し（CREATE_NO_WINDOW）、stdout+stderr をまとめて
/// 捕捉する。成功時は (出力バイト列, 終了コード)、起動失敗時は Err(GetLastError)。
unsafe fn run_capture(exe_path: &[u16], args: &str) -> Result<(Vec<u8>, u32), u32> {
    let mut sa: SECURITY_ATTRIBUTES = mem::zeroed();
    sa.nLength = mem::size_of::<SECURITY_ATTRIBUTES>() as u32;
    sa.bInheritHandle = 1; // TRUE

    let mut h_read: HANDLE = ptr::null_mut();
    let mut h_write: HANDLE = ptr::null_mut();
    if CreatePipe(&mut h_read, &mut h_write, &sa, 0) == 0 {
        return Err(GetLastError());
    }
    // 子には書き込み側だけを継承させる（読み取り側は継承させない）。
    SetHandleInformation(h_read, HANDLE_FLAG_INHERIT, 0);

    // stdin は NUL デバイス。開けなければパイプにフォールバック
    // （書き込み側は親が保持し、起動直後に閉じるので子は EOF を見る）。
    let nul = wide("NUL");
    let mut h_std_in = CreateFileW(
        nul.as_ptr(),
        GENERIC_READ,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        &sa,
        OPEN_EXISTING,
        0,
        ptr::null_mut(),
    );
    let mut h_in_write: HANDLE = ptr::null_mut();
    if h_std_in == INVALID_HANDLE_VALUE {
        if CreatePipe(&mut h_std_in, &mut h_in_write, &sa, 0) != 0 {
            SetHandleInformation(h_in_write, HANDLE_FLAG_INHERIT, 0);
        } else {
            let err = GetLastError();
            CloseHandle(h_read);
            CloseHandle(h_write);
            return Err(err);
        }
    }

    let mut si: STARTUPINFOW = mem::zeroed();
    si.cb = mem::size_of::<STARTUPINFOW>() as u32;
    si.dwFlags = STARTF_USESTDHANDLES;
    si.hStdInput = h_std_in;
    si.hStdOutput = h_write;
    si.hStdError = h_write;
    let mut pi: PROCESS_INFORMATION = mem::zeroed();

    // コマンドライン: 先頭に引用符付きの実行ファイルパスを置き、
    // lpApplicationName にも同じパスを渡す二重指定（解析依存を避ける）。
    // CreateProcessW はバッファを書き換えることがあるため、必ず自前の
    // 可変バッファを渡す。
    let exe_str = String::from_utf16_lossy(exe_path);
    let cmd_line = if args.is_empty() {
        format!("\"{}\"", exe_str)
    } else {
        format!("\"{}\" {}", exe_str, args)
    };
    let mut cmd_buf = wide(&cmd_line);
    let exe_z = zterm(exe_path);

    let ok = CreateProcessW(
        exe_z.as_ptr(),
        cmd_buf.as_mut_ptr(),
        ptr::null(),
        ptr::null(),
        1, // bInheritHandles = TRUE
        CREATE_NO_WINDOW,
        ptr::null(),
        ptr::null(),
        &si,
        &mut pi,
    );
    // 失敗時のエラーコードは、他の API を呼ぶ前に捕捉する。
    let last_err = if ok == 0 { GetLastError() } else { 0 };

    // 子が自分のコピーを持ったので、親側の端は閉じる。
    CloseHandle(h_write);
    CloseHandle(h_std_in);
    if !h_in_write.is_null() {
        CloseHandle(h_in_write);
    }
    if ok == 0 {
        CloseHandle(h_read);
        return Err(last_err);
    }

    let mut data: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];

    // ポーリング: たまっている分を読み、終わるまで待つ間は
    // メッセージポンプを回して画面を固まらせない。
    loop {
        let mut avail: u32 = 0;
        if PeekNamedPipe(
            h_read,
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            &mut avail,
            ptr::null_mut(),
        ) == 0
        {
            break;
        }
        if avail > 0 {
            let mut got: u32 = 0;
            if ReadFile(
                h_read,
                chunk.as_mut_ptr(),
                chunk.len() as u32,
                &mut got,
                ptr::null_mut(),
            ) != 0
                && got > 0
            {
                data.extend_from_slice(&chunk[..got as usize]);
            }
        } else {
            let mut code: u32 = 0;
            GetExitCodeProcess(pi.hProcess, &mut code);
            if code != STILL_ACTIVE_U32 {
                break;
            }
            let mut msg: MSG = mem::zeroed();
            while PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            Sleep(15);
        }
    }
    // 最後の吸い出し（書き手がいなくなればブロック読みは EOF で終わる）。
    loop {
        let mut got: u32 = 0;
        if ReadFile(
            h_read,
            chunk.as_mut_ptr(),
            chunk.len() as u32,
            &mut got,
            ptr::null_mut(),
        ) == 0
        {
            break;
        }
        if got == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..got as usize]);
    }
    WaitForSingleObject(pi.hProcess, INFINITE);
    let mut exit_code: u32 = 0;
    GetExitCodeProcess(pi.hProcess, &mut exit_code);
    CloseHandle(pi.hThread);
    CloseHandle(pi.hProcess);
    CloseHandle(h_read);
    Ok((data, exit_code))
}

/* ---------- UI アクション ---------- */

unsafe fn do_browse() {
    let mut file_buf = [0u16; 2048];
    // フィルタは NUL 区切りの組＋二重 NUL 終端（Delphi 版と同じ組み立て）。
    let mut filter: Vec<u16> = Vec::new();
    filter.extend_from_slice(&utf16("音声ファイル (*.mp3; *.wav)"));
    filter.push(0);
    filter.extend_from_slice(&utf16("*.mp3;*.wav"));
    filter.push(0);
    filter.extend_from_slice(&utf16("すべてのファイル (*.*)"));
    filter.push(0);
    filter.extend_from_slice(&utf16("*.*"));
    filter.push(0);
    filter.push(0);
    let title = wide("音声ファイルを選択");

    let mut ofn: OPENFILENAMEW = mem::zeroed();
    ofn.lStructSize = mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = H_MAIN;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file_buf.as_mut_ptr();
    ofn.nMaxFile = file_buf.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY;
    if GetOpenFileNameW(&mut ofn) != 0 {
        SetWindowTextW(H_EDIT, file_buf.as_ptr());
        SendMessageW(H_LIST, LB_RESETCONTENT, 0, 0);
        set_status("ファイルが選択されました。「無音を検出」を押してください。");
    }
}

unsafe fn do_detect() {
    // 入力欄のパスを取得（Delphi 版と同じ 2048 文字バッファ）。
    let mut path_buf = [0u16; 2048];
    let path_len = GetWindowTextW(H_EDIT, path_buf.as_mut_ptr(), path_buf.len() as i32) as usize;
    if path_len == 0 {
        set_status("先に「参照」ボタンで音声ファイルを選択してください。");
        return;
    }
    let file_path: Vec<u16> = path_buf[..path_len].to_vec();
    let file_path_str = String::from_utf16_lossy(&file_path);

    let ffmpeg_path = match find_ffmpeg() {
        Some(p) => p,
        None => {
            set_status("ffmpeg.exe が見つかりませんでした。");
            message_box(
                "ffmpeg.exe が見つかりませんでした。\r\n次のいずれかの場所に ffmpeg.exe を置いてください：\r\n・このアプリと同じフォルダ\r\n・%LocalAppData%\\Mp3SilenceCutter\\bin（本編アプリの展開先）\r\n・PATH の通ったフォルダ",
                "ffmpeg が見つかりません",
                MB_OK | MB_ICONWARNING,
            );
            return;
        }
    };

    // 起動直前の実在チェック: 使おうとしている文字列そのものが
    // 実在ファイルかを確認する（Delphi 版・第3回修正で追加された関門）。
    if !file_exists(&ffmpeg_path) {
        let msg = format!(
            "ffmpegの場所は見つかりましたが、ファイルにアクセスできません：{}",
            String::from_utf16_lossy(&ffmpeg_path)
        );
        set_status(&msg);
        message_box(&msg, "ffmpeg にアクセスできません", MB_OK | MB_ICONERROR);
        return;
    }

    EnableWindow(H_DETECT, 0);
    EnableWindow(H_BROWSE, 0);
    // どの ffmpeg を使うかを先に表示する（画面だけで状況が分かるように）。
    let ffmpeg_str = String::from_utf16_lossy(&ffmpeg_path);
    set_status(&format!("検出中です… ffmpeg: {}", ffmpeg_str));
    UpdateWindow(H_STATUS);
    SendMessageW(H_LIST, LB_RESETCONTENT, 0, 0);

    // 引数の形は本編（C#版）・Delphi 版と同一: -f null -
    let args = format!(
        "-nostdin -i \"{}\" -af silencedetect=noise=-45dB:d=1.0 -f null -",
        file_path_str
    );
    let result = run_capture(&ffmpeg_path, &args);
    // Delphi 版の finally 相当: どの経路でもボタンは必ず再有効化する。
    EnableWindow(H_DETECT, 1);
    EnableWindow(H_BROWSE, 1);

    let (log, exit_code) = match result {
        Ok(v) => v,
        Err(last_err) => {
            let text = win_error_text(last_err);
            let msg = if text.is_empty() {
                format!("ffmpeg を起動できませんでした（エラー {}）", last_err)
            } else {
                format!(
                    "ffmpeg を起動できませんでした（エラー {}：{}）",
                    last_err, text
                )
            };
            set_status(&msg);
            message_box(
                &format!("{}\r\nパス： {}", msg, ffmpeg_str),
                "ffmpeg を起動できません",
                MB_OK | MB_ICONERROR,
            );
            return;
        }
    };

    let ranges = parse_silence_detect_log(&log);
    let mut total = 0.0f64;
    for (i, r) in ranges.iter().enumerate() {
        let dur = r.end_sec - r.start_sec;
        total += dur;
        let line = format!(
            "{})  {} ～ {} 秒  （長さ {} 秒）",
            i + 1,
            fmt_sec(r.start_sec),
            fmt_sec(r.end_sec),
            fmt_sec(dur)
        );
        let w = wide(&line);
        SendMessageW(H_LIST, LB_ADDSTRING, 0, w.as_ptr() as LPARAM);
    }
    if ranges.is_empty() {
        if exit_code != 0 {
            set_status(&format!(
                "無音区間は検出されませんでした（ffmpeg 終了コード {}）。",
                exit_code
            ));
        } else {
            set_status("無音区間は検出されませんでした。");
        }
    } else {
        set_status(&format!(
            "無音区間： {} 件 ／ 無音の合計： {} 秒",
            ranges.len(),
            fmt_sec(total)
        ));
    }
}

/* ---------- ウィンドウ ---------- */

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_COMMAND => {
            if hiword(wparam) == BN_CLICKED {
                match loword(wparam) {
                    IDC_BROWSE => do_browse(),
                    IDC_DETECT => do_detect(),
                    _ => {}
                }
            }
            0
        }
        WM_SIZE => {
            let mut w = loword(lparam as usize) as i32;
            let mut h = hiword(lparam as usize) as i32;
            if w < 200 {
                w = 200;
            }
            if h < 200 {
                h = 200;
            }
            MoveWindow(H_EDIT, 12, 12, w - 24 - 88 - 8, 26, 1);
            MoveWindow(H_BROWSE, w - 12 - 88, 12, 88, 26, 1);
            MoveWindow(H_DETECT, 12, 50, 150, 32, 1);
            MoveWindow(H_STATUS, 174, 50, w - 174 - 12, 32, 1);
            MoveWindow(H_LIST, 12, 94, w - 24, h - 94 - 12, 1);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub fn run() {
    unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(ptr::null());
        let class_name = wide(CLASS_NAME);
        let app_title = wide(APP_TITLE);
        let gui_font: HFONT = GetStockObject(DEFAULT_GUI_FONT) as HFONT;

        let mut wc: WNDCLASSEXW = mem::zeroed();
        wc.cbSize = mem::size_of::<WNDCLASSEXW>() as u32;
        wc.style = CS_HREDRAW | CS_VREDRAW;
        wc.lpfnWndProc = Some(wnd_proc);
        wc.hInstance = hinstance;
        wc.hCursor = LoadCursorW(ptr::null_mut(), IDC_ARROW);
        wc.hbrBackground = (COLOR_BTNFACE + 1) as isize as HBRUSH;
        wc.lpszClassName = class_name.as_ptr();
        if RegisterClassExW(&wc) == 0 {
            let msg = wide("ウィンドウクラスの登録に失敗しました。");
            let cap = wide("エラー");
            MessageBoxW(ptr::null_mut(), msg.as_ptr(), cap.as_ptr(), MB_OK | MB_ICONERROR);
            std::process::exit(1);
        }

        H_MAIN = CreateWindowExW(
            0,
            class_name.as_ptr(),
            app_title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            680,
            460,
            ptr::null_mut(),
            ptr::null_mut(),
            hinstance,
            ptr::null(),
        );
        if H_MAIN.is_null() {
            let msg = wide("ウィンドウの作成に失敗しました。");
            let cap = wide("エラー");
            MessageBoxW(ptr::null_mut(), msg.as_ptr(), cap.as_ptr(), MB_OK | MB_ICONERROR);
            std::process::exit(1);
        }

        let edit_class = wide("EDIT");
        let button_class = wide("BUTTON");
        let static_class = wide("STATIC");
        let listbox_class = wide("LISTBOX");
        let browse_text = wide("参照");
        let detect_text = wide("無音を検出");
        let status_text = wide("音声ファイルを選択してください。");

        H_EDIT = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            edit_class.as_ptr(),
            ptr::null(),
            WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL as u32,
            12,
            12,
            460,
            26,
            H_MAIN,
            IDC_PATH as HMENU,
            hinstance,
            ptr::null(),
        );
        H_BROWSE = CreateWindowExW(
            0,
            button_class.as_ptr(),
            browse_text.as_ptr(),
            WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON as u32,
            480,
            12,
            88,
            26,
            H_MAIN,
            IDC_BROWSE as HMENU,
            hinstance,
            ptr::null(),
        );
        H_DETECT = CreateWindowExW(
            0,
            button_class.as_ptr(),
            detect_text.as_ptr(),
            WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON as u32,
            12,
            50,
            150,
            32,
            H_MAIN,
            IDC_DETECT as HMENU,
            hinstance,
            ptr::null(),
        );
        H_STATUS = CreateWindowExW(
            0,
            static_class.as_ptr(),
            status_text.as_ptr(),
            WS_CHILD | WS_VISIBLE | SS_CENTERIMAGE,
            174,
            50,
            480,
            32,
            H_MAIN,
            IDC_STATUS as HMENU,
            hinstance,
            ptr::null(),
        );
        H_LIST = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            listbox_class.as_ptr(),
            ptr::null(),
            WS_CHILD | WS_VISIBLE | WS_VSCROLL | LBS_NOTIFY as u32,
            12,
            94,
            644,
            340,
            H_MAIN,
            IDC_LIST as HMENU,
            hinstance,
            ptr::null(),
        );

        for h in [H_EDIT, H_BROWSE, H_DETECT, H_STATUS, H_LIST] {
            SendMessageW(h, WM_SETFONT, gui_font as WPARAM, 1);
        }

        ShowWindow(H_MAIN, SW_SHOW);
        UpdateWindow(H_MAIN);

        let mut msg: MSG = mem::zeroed();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        std::process::exit(msg.wParam as i32);
    }
}
