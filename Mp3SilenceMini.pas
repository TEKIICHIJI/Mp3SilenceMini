program Mp3SilenceMini;
{ MP3無音検出ミニ (Delphi実験版)
  Minimal experiment: pick an audio file, run ffmpeg silencedetect
  (noise=-45dB, d=1.0, fixed) and list the silence ranges.
  Raw Win32 API (Unicode "W" functions), single program file.
  Built with Free Pascal (Delphi-compatible Object Pascal), native win64. }

{$mode delphi}
{$apptype gui}
{$codepage utf8}

uses
  Windows, CommDlg, StrUtils, SysUtils;

{$I SilenceParse.inc}

const
  IDC_PATH   = 101;
  IDC_BROWSE = 102;
  IDC_DETECT = 103;
  IDC_STATUS = 104;
  IDC_LIST   = 105;
  CLASS_NAME = 'Mp3SilenceMiniClass';

var
  hMain    : HWND = 0;
  hEdit    : HWND = 0;
  hBrowse  : HWND = 0;
  hDetect  : HWND = 0;
  hStatus  : HWND = 0;
  hList    : HWND = 0;
  hGUIFont : HFONT = 0;

{ ---------- small helpers ---------- }

function FmtSec(x: Double): WideString;
var
  fs : TFormatSettings;
begin
  fs := DefaultFormatSettings;
  fs.DecimalSeparator := '.';
  Result := FormatFloat('0.000', x, fs);
end;

procedure SetStatus(const S: WideString);
begin
  SetWindowTextW(hStatus, PWideChar(S));
end;

function FileExistsW(const Path: WideString): Boolean;
var
  Attr : DWORD;
begin
  Attr := GetFileAttributesW(PWideChar(Path));
  Result := (Attr <> INVALID_FILE_ATTRIBUTES) and ((Attr and FILE_ATTRIBUTE_DIRECTORY) = 0);
end;

{ Build a WideString from a WideChar buffer with an explicit character
  count, via SetLength + Move -- no overload resolution involved.
  NEVER write SetString(S, @Buf[0], N) for this: with typed-address
  checking off (the FPC default), @Buf[0] is an UNTYPED Pointer, and
  SetString then resolves to its PAnsiChar overload, widening the buffer
  BYTE BY BYTE.
  The result is 'I',#0,':',#0,... -- every C API sees only the first
  character. That was the round-3 bug: the ffmpeg path became "I",
  steps (1)(2) failed their existence check and step (3) handed the
  one-character path to CreateProcess (error 2). Passing a PWideChar
  variable or an explicit PWideChar(...) cast picks the right overload;
  this helper avoids the trap entirely. }
function WideFromBuf(Buf: PWideChar; Len: Integer): WideString;
begin
  Result := '';
  if (Buf = nil) or (Len <= 0) then Exit;
  SetLength(Result, Len);
  Move(Buf^, Result[1], Len * SizeOf(WideChar));
end;

{ ffmpeg discovery: (1) next to the exe, (2) the main app's extracted copy,
  (3) PATH. Returns '' when not found.
  The whole lookup is exception-guarded: discovery must never crash the app. }
function FindFFmpeg: WideString;
var
  Buf : array[0..1023] of WideChar;
  Dir, Candidate : WideString;
  NameW, EnvW : WideString;
  N : DWORD;
  PartPtr : PWideChar;
begin
  Result := '';
  try
    { (1) same folder as this exe (pure WideString handling, no codepage loss) }
    N := GetModuleFileNameW(0, @Buf[0], Length(Buf));
    if N > 0 then
    begin
      Dir := WideFromBuf(@Buf[0], Integer(N));
      while (Dir <> '') and (Dir[Length(Dir)] <> '\') do
        Delete(Dir, Length(Dir), 1);
      Candidate := Dir + 'ffmpeg.exe';
      if FileExistsW(Candidate) then Exit(Candidate);
    end;
    { (2) %LocalAppData%\Mp3SilenceCutter\bin\ffmpeg.exe }
    EnvW := 'LOCALAPPDATA';
    N := GetEnvironmentVariableW(PWideChar(EnvW), @Buf[0], Length(Buf));
    if (N > 0) and (N < DWORD(Length(Buf))) then
    begin
      Dir := WideFromBuf(@Buf[0], Integer(N));
      Candidate := Dir + '\Mp3SilenceCutter\bin\ffmpeg.exe';
      if FileExistsW(Candidate) then Exit(Candidate);
    end;
    { (3) PATH }
    { SearchPathW's last parameter is "LPWSTR *lpPart": the API WRITES the
      address of the file-name part through the pointer we pass. It must be
      the ADDRESS of a PWideChar variable (@PartPtr), never the variable's
      (uninitialised) value -- passing the value makes the API write through
      a wild pointer and raises EAccessViolation inside the system DLL.
      That was the crash on the detect path; fixed 2026-11-22. }
    PartPtr := nil;
    NameW := 'ffmpeg.exe';
    N := SearchPathW(nil, PWideChar(NameW), nil, Length(Buf), @Buf[0], @PartPtr);
    if (N > 0) and (N < DWORD(Length(Buf))) then
    begin
      Result := WideFromBuf(@Buf[0], Integer(N));
    end;
  except
    Result := '';
  end;
end;

{ Human-readable system text for a GetLastError code ('' when unknown).
  Trailing punctuation/line breaks are trimmed for embedding in a sentence. }
function WinErrorText(Code: DWORD): WideString;
var
  Buf : PWideChar;
  N : DWORD;
  S : WideString;
  LastOrd : Integer;
begin
  Result := '';
  Buf := nil;
  N := FormatMessageW(FORMAT_MESSAGE_ALLOCATE_BUFFER or FORMAT_MESSAGE_FROM_SYSTEM or
                      FORMAT_MESSAGE_IGNORE_INSERTS, nil, Code,
                      MAKELANGID(LANG_NEUTRAL, SUBLANG_DEFAULT),
                      LPWSTR(@Buf), 0, nil);
  if (N > 0) and (Buf <> nil) then
  begin
    S := WideFromBuf(Buf, Integer(N));
    LocalFree(HLOCAL(Buf));
    while S <> '' do
    begin
      LastOrd := Ord(S[Length(S)]);
      if (LastOrd = 13) or (LastOrd = 10) or (LastOrd = 32) or
         (LastOrd = $2E) or (LastOrd = $3002) then
        Delete(S, Length(S), 1)
      else
        Break;
    end;
    Result := S;
  end;
end;

{ Run ExePath with Args (CREATE_NO_WINDOW), capturing combined stdout+stderr.
  Round-2 fix: the executable is passed BOTH as lpApplicationName and as the
  quoted first token of the command line, so CreateProcess never has to parse
  the command line to locate the program -- with lpApplicationName = nil a
  quoted first token is a classic launch-failure mode. stdin is the NUL
  device; if it cannot be opened, a pipe whose write end stays in the parent
  (closed right after launch, so the child sees EOF) is used instead: with
  STARTF_USESTDHANDLES every std handle must be a valid inheritable handle,
  and handing over 0 / INVALID_HANDLE_VALUE makes CreateProcess fail with
  ERROR_INVALID_HANDLE. On failure LastErr carries the GetLastError code
  captured immediately after the failing API call.
  Polls with a message pump so the window stays responsive. }
function RunCapture(const ExePath, Args: WideString;
                    out Output: AnsiString; out ExitCode: DWORD;
                    out LastErr: DWORD): Boolean;
var
  sa : SECURITY_ATTRIBUTES;
  hRead, hWrite, hStdIn, hInWrite : THandle;
  si : STARTUPINFOW;
  pi : PROCESS_INFORMATION;
  CmdLine : WideString;
  cmdBuf : array of WideChar;
  data : array of Byte;
  chunk : array[0..4095] of Byte;
  avail, got, oldLen, code : DWORD;
  ok : Boolean;
  msg : TMsg;
  NulW : WideString;
begin
  Result := False;
  Output := '';
  ExitCode := 0;
  LastErr := 0;
  FillChar(sa, SizeOf(sa), 0);
  sa.nLength := SizeOf(sa);
  sa.bInheritHandle := True;
  if not CreatePipe(hRead, hWrite, @sa, 0) then
  begin
    LastErr := GetLastError;
    Exit;
  end;
  { The child inherits the write end only, never the read end of its pipe. }
  SetHandleInformation(hRead, HANDLE_FLAG_INHERIT, 0);
  NulW := 'NUL';
  hStdIn := CreateFileW(PWideChar(NulW), GENERIC_READ, FILE_SHARE_READ or FILE_SHARE_WRITE,
                        @sa, OPEN_EXISTING, 0, 0);
  hInWrite := 0;
  if hStdIn = INVALID_HANDLE_VALUE then
  begin
    if CreatePipe(hStdIn, hInWrite, @sa, 0) then
      SetHandleInformation(hInWrite, HANDLE_FLAG_INHERIT, 0)
    else
    begin
      LastErr := GetLastError;
      CloseHandle(hRead);
      CloseHandle(hWrite);
      Exit;
    end;
  end;
  FillChar(si, SizeOf(si), 0);
  si.cb := SizeOf(si);
  si.dwFlags := STARTF_USESTDHANDLES;
  si.hStdInput  := hStdIn;
  si.hStdOutput := hWrite;
  si.hStdError  := hWrite;
  FillChar(pi, SizeOf(pi), 0);
  CmdLine := '"' + ExePath + '"';
  if Args <> '' then
    CmdLine := CmdLine + ' ' + Args;
  { CreateProcessW may write into the command-line buffer: always hand it
    our own writable copy, never the string's own storage. }
  SetLength(cmdBuf, Length(CmdLine) + 1);
  if Length(CmdLine) > 0 then
    Move(PWideChar(CmdLine)^, cmdBuf[0], (Length(CmdLine) + 1) * SizeOf(WideChar))
  else
    cmdBuf[0] := #0;
  ok := CreateProcessW(PWideChar(ExePath), @cmdBuf[0], nil, nil, True,
                       CREATE_NO_WINDOW, nil, nil, si, pi);
  if not ok then
    LastErr := GetLastError;  { capture BEFORE any other API call }
  { The child owns its copies of the handles now; drop the parent's ends. }
  CloseHandle(hWrite);
  CloseHandle(hStdIn);
  if hInWrite <> 0 then CloseHandle(hInWrite);
  if not ok then
  begin
    CloseHandle(hRead);
    Exit;
  end;
  SetLength(data, 0);
  { poll loop: drain what is available while the process runs }
  while True do
  begin
    avail := 0;
    if not PeekNamedPipe(hRead, nil, 0, nil, @avail, nil) then Break;
    if avail > 0 then
    begin
      got := 0;
      if ReadFile(hRead, chunk[0], SizeOf(chunk), got, nil) and (got > 0) then
      begin
        oldLen := Length(data);
        SetLength(data, oldLen + got);
        Move(chunk[0], data[oldLen], got);
      end;
    end
    else
    begin
      code := 0;
      GetExitCodeProcess(pi.hProcess, code);
      if code <> STILL_ACTIVE then Break;
      while PeekMessageW(msg, 0, 0, 0, PM_REMOVE) do
      begin
        TranslateMessage(msg);
        DispatchMessageW(msg);
      end;
      Sleep(15);
    end;
  end;
  { final drain (writer is gone or pipe broke; blocking read ends at EOF) }
  while True do
  begin
    got := 0;
    if not ReadFile(hRead, chunk[0], SizeOf(chunk), got, nil) then Break;
    if got = 0 then Break;
    oldLen := Length(data);
    SetLength(data, oldLen + got);
    Move(chunk[0], data[oldLen], got);
  end;
  WaitForSingleObject(pi.hProcess, INFINITE);
  GetExitCodeProcess(pi.hProcess, ExitCode);
  CloseHandle(pi.hThread);
  CloseHandle(pi.hProcess);
  CloseHandle(hRead);
  if Length(data) > 0 then
    SetString(Output, PAnsiChar(@data[0]), Length(data));
  Result := True;
end;

{ ---------- UI actions ---------- }

procedure DoBrowse;
var
  ofn : OPENFILENAMEW;
  FileBuf : array[0..2047] of WideChar;
  Filter, TitleW : WideString;
begin
  FillChar(ofn, SizeOf(ofn), 0);
  FillChar(FileBuf, SizeOf(FileBuf), 0);
  Filter := WideString('音声ファイル (*.mp3; *.wav)') + #0 + WideString('*.mp3;*.wav') + #0 +
            WideString('すべてのファイル (*.*)') + #0 + WideString('*.*') + #0 + #0;
  ofn.lStructSize := SizeOf(ofn);
  ofn.hwndOwner := hMain;
  ofn.lpstrFilter := PWideChar(Filter);
  ofn.lpstrFile := @FileBuf[0];
  ofn.nMaxFile := Length(FileBuf);
  TitleW := '音声ファイルを選択';
  ofn.lpstrTitle := PWideChar(TitleW);
  ofn.Flags := OFN_FILEMUSTEXIST or OFN_PATHMUSTEXIST or OFN_HIDEREADONLY;
  if GetOpenFileNameW(@ofn) then
  begin
    SetWindowTextW(hEdit, @FileBuf[0]);
    SendMessageW(hList, LB_RESETCONTENT, 0, 0);
    SetStatus('ファイルが選択されました。「無音を検出」を押してください。');
  end;
end;

procedure DoDetect;
var
  PathBuf : array[0..2047] of WideChar;
  FilePath, FFmpegPath, Args, ErrText, ErrMsg : WideString;
  LogText : AnsiString;
  ExitCode, LastErr : DWORD;
  Ranges : TSilenceRangeArray;
  Count, i, PathLen : Integer;
  TotalSilence : Double;
  Line : WideString;
begin
  { Safety net: ANY failure on the detect path must surface as a polite
    Japanese message, never as an unhandled-exception crash dialog. }
  try
  PathLen := GetWindowTextW(hEdit, @PathBuf[0], Length(PathBuf));
  if PathLen = 0 then
  begin
    SetStatus('先に「参照」ボタンで音声ファイルを選択してください。');
    Exit;
  end;
  FilePath := WideFromBuf(@PathBuf[0], PathLen);
  FFmpegPath := FindFFmpeg;
  if FFmpegPath = '' then
  begin
    SetStatus('ffmpeg.exe が見つかりませんでした。');
    MessageBoxW(hMain,
      PWideChar(WideString('ffmpeg.exe が見つかりませんでした。' + #13#10 +
        '次のいずれかの場所に ffmpeg.exe を置いてください：' + #13#10 +
        '・このアプリと同じフォルダ' + #13#10 +
        '・%LocalAppData%\Mp3SilenceCutter\bin（本編アプリの展開先）' + #13#10 +
        '・PATH の通ったフォルダ')),
      PWideChar(WideString('ffmpeg が見つかりません')),
      MB_OK or MB_ICONWARNING);
    Exit;
  end;
  { Pre-launch sanity check: the exact path string about to be used must
    name a real, reachable file. (Round 3: a string-handling bug once
    reduced this path to "I" and the failure surfaced only as a bare
    CreateProcess error 2. Checking the string itself catches any such
    corruption here, with a distinct message.) }
  if not FileExistsW(FFmpegPath) then
  begin
    ErrMsg := 'ffmpegの場所は見つかりましたが、ファイルにアクセスできません：' + FFmpegPath;
    SetStatus(ErrMsg);
    MessageBoxW(hMain,
      PWideChar(ErrMsg),
      PWideChar(WideString('ffmpeg にアクセスできません')),
      MB_OK or MB_ICONERROR);
    Exit;
  end;
  EnableWindow(hDetect, False);
  EnableWindow(hBrowse, False);
  { Show WHICH ffmpeg is about to run, so any screenshot is self-diagnosing. }
  SetStatus('検出中です… ffmpeg: ' + FFmpegPath);
  UpdateWindow(hStatus);
  SendMessageW(hList, LB_RESETCONTENT, 0, 0);
  { Same argument shape as the known-good main (C#) app: -f null - }
  Args := '-nostdin -i "' + FilePath +
          '" -af silencedetect=noise=-45dB:d=1.0 -f null -';
  try
    if not RunCapture(FFmpegPath, Args, LogText, ExitCode, LastErr) then
    begin
      ErrText := WinErrorText(LastErr);
      if ErrText <> '' then
        ErrMsg := 'ffmpeg を起動できませんでした（エラー ' + IntToStr(LastErr) +
                  '：' + ErrText + '）'
      else
        ErrMsg := 'ffmpeg を起動できませんでした（エラー ' + IntToStr(LastErr) + '）';
      SetStatus(ErrMsg);
      MessageBoxW(hMain,
        PWideChar(ErrMsg + #13#10 + 'パス： ' + FFmpegPath),
        PWideChar(WideString('ffmpeg を起動できません')),
        MB_OK or MB_ICONERROR);
      Exit;
    end;
  finally
    EnableWindow(hDetect, True);
    EnableWindow(hBrowse, True);
  end;
  Count := ParseSilenceDetectLog(string(LogText), Ranges);
  TotalSilence := 0;
  for i := 0 to Count - 1 do
  begin
    TotalSilence := TotalSilence + (Ranges[i].EndSec - Ranges[i].StartSec);
    Line := IntToStr(i + 1) + ')  ' + FmtSec(Ranges[i].StartSec) + ' ～ ' +
            FmtSec(Ranges[i].EndSec) + ' 秒  （長さ ' +
            FmtSec(Ranges[i].EndSec - Ranges[i].StartSec) + ' 秒）';
    SendMessageW(hList, LB_ADDSTRING, 0, LPARAM(PWideChar(Line)));
  end;
  if Count = 0 then
  begin
    if ExitCode <> 0 then
      SetStatus('無音区間は検出されませんでした（ffmpeg 終了コード ' + IntToStr(ExitCode) + '）。')
    else
      SetStatus('無音区間は検出されませんでした。');
  end
  else
    SetStatus('無音区間： ' + IntToStr(Count) + ' 件 ／ 無音の合計： ' + FmtSec(TotalSilence) + ' 秒');
  except
    on E: Exception do
    begin
      EnableWindow(hDetect, True);
      EnableWindow(hBrowse, True);
      SetStatus('検出中にエラーが発生しました。');
      MessageBoxW(hMain,
        PWideChar(WideString('検出中にエラーが発生しました。' + #13#10 +
          '内容： ' + WideString(E.Message) + #13#10 +
          'お手数ですが、もう一度お試しください。')),
        PWideChar(WideString('エラー')),
        MB_OK or MB_ICONERROR);
    end;
  end;
end;

{ ---------- window ---------- }

function WndProc(hWnd: HWND; uMsg: UINT; wParam: WPARAM; lParam: LPARAM): LRESULT; stdcall;
var
  W, H : Integer;
begin
  case uMsg of
    WM_COMMAND:
      begin
        if (HIWORD(wParam) = BN_CLICKED) then
        begin
          case LOWORD(wParam) of
            IDC_BROWSE: DoBrowse;
            IDC_DETECT: DoDetect;
          end;
        end;
        Result := 0;
      end;
    WM_SIZE:
      begin
        W := LOWORD(lParam);
        H := HIWORD(lParam);
        if W < 200 then W := 200;
        if H < 200 then H := 200;
        MoveWindow(hEdit,   12, 12, W - 24 - 88 - 8, 26, True);
        MoveWindow(hBrowse, W - 12 - 88, 12, 88, 26, True);
        MoveWindow(hDetect, 12, 50, 150, 32, True);
        MoveWindow(hStatus, 174, 50, W - 174 - 12, 32, True);
        MoveWindow(hList,   12, 94, W - 24, H - 94 - 12, True);
        Result := 0;
      end;
    WM_DESTROY:
      begin
        PostQuitMessage(0);
        Result := 0;
      end;
  else
    Result := DefWindowProcW(hWnd, uMsg, wParam, lParam);
  end;
end;

var
  wc : WNDCLASSEXW;
  Msg : TMsg;
  AppTitle, ClassNameW : WideString;
begin
  AppTitle := 'MP3無音検出ミニ（Delphi実験版）';
  ClassNameW := CLASS_NAME;
  hGUIFont := GetStockObject(DEFAULT_GUI_FONT);
  FillChar(wc, SizeOf(wc), 0);
  wc.cbSize := SizeOf(wc);
  wc.style := CS_HREDRAW or CS_VREDRAW;
  wc.lpfnWndProc := @WndProc;
  wc.hInstance := HInstance;
  wc.hCursor := LoadCursorW(0, PWideChar(IDC_ARROW));
  wc.hbrBackground := HBRUSH(COLOR_BTNFACE + 1);
  wc.lpszClassName := PWideChar(ClassNameW);
  if RegisterClassExW(@wc) = 0 then
  begin
    MessageBoxW(0, 'ウィンドウクラスの登録に失敗しました。', 'エラー', MB_OK or MB_ICONERROR);
    Halt(1);
  end;
  hMain := CreateWindowExW(0, PWideChar(ClassNameW), PWideChar(AppTitle),
    WS_OVERLAPPEDWINDOW, CW_USEDEFAULT, CW_USEDEFAULT, 680, 460,
    0, 0, HInstance, nil);
  if hMain = 0 then
  begin
    MessageBoxW(0, 'ウィンドウの作成に失敗しました。', 'エラー', MB_OK or MB_ICONERROR);
    Halt(1);
  end;

  hEdit := CreateWindowExW(WS_EX_CLIENTEDGE, 'EDIT', nil,
    WS_CHILD or WS_VISIBLE or ES_AUTOHSCROLL, 12, 12, 460, 26, hMain, HMENU(IDC_PATH), HInstance, nil);
  hBrowse := CreateWindowExW(0, 'BUTTON', PWideChar(WideString('参照')),
    WS_CHILD or WS_VISIBLE or BS_PUSHBUTTON, 480, 12, 88, 26, hMain, HMENU(IDC_BROWSE), HInstance, nil);
  hDetect := CreateWindowExW(0, 'BUTTON', PWideChar(WideString('無音を検出')),
    WS_CHILD or WS_VISIBLE or BS_PUSHBUTTON, 12, 50, 150, 32, hMain, HMENU(IDC_DETECT), HInstance, nil);
  hStatus := CreateWindowExW(0, 'STATIC', PWideChar(WideString('音声ファイルを選択してください。')),
    WS_CHILD or WS_VISIBLE or SS_CENTERIMAGE, 174, 50, 480, 32, hMain, HMENU(IDC_STATUS), HInstance, nil);
  hList := CreateWindowExW(WS_EX_CLIENTEDGE, 'LISTBOX', nil,
    WS_CHILD or WS_VISIBLE or WS_VSCROLL or LBS_NOTIFY, 12, 94, 644, 340, hMain, HMENU(IDC_LIST), HInstance, nil);

  SendMessageW(hEdit,   WM_SETFONT, WPARAM(hGUIFont), 1);
  SendMessageW(hBrowse, WM_SETFONT, WPARAM(hGUIFont), 1);
  SendMessageW(hDetect, WM_SETFONT, WPARAM(hGUIFont), 1);
  SendMessageW(hStatus, WM_SETFONT, WPARAM(hGUIFont), 1);
  SendMessageW(hList,   WM_SETFONT, WPARAM(hGUIFont), 1);

  ShowWindow(hMain, SW_SHOW);
  UpdateWindow(hMain);
  while GetMessageW(Msg, 0, 0, 0) do
  begin
    TranslateMessage(Msg);
    DispatchMessageW(Msg);
  end;
  Halt(Integer(Msg.wParam));
end.
