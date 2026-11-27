# Mp3SilenceMini（Rust実験版）ビルドレポート

目的： Delphi実験版（Free Pascal・生 Win32）と 1:1 で比べるための Rust 版。
仕様・文言・ffmpeg の呼び出し方は Delphi 版に一致させ、違うのはタイトルが
「Rust実験版」になることだけ（2026-11-27、ナツの新実験）。

## 成果物

| 項目 | 値 |
|---|---|
| exe | `~/workspace/your_files/Mp3SilenceMini-Rust.exe` |
| exe サイズ | **292,864 バイト**（約 286 KB） |
| exe SHA256 | `e790135162608f7d518f346138d94b6317a380bdd4bf9d124e050afce505b4fa` |
| zip | `~/workspace/your_files/Mp3SilenceMini-Rust.zip`（中身は exe のみ、ffmpeg は含まない） |
| zip サイズ | **154,618 バイト** |
| zip SHA256 | `c3c46fc5d8d33bd604a46ff8b0f54d0e08d6541b0cccf9c990c04e83ef143e86` |

### Delphi実験版とのサイズ比較

| | exe | zip |
|---|---|---|
| Delphi実験版（Free Pascal、生 Win32） | 143,360 バイト | 58,720 バイト |
| **Rust実験版（本ビルド）** | **292,864 バイト（約 2.04 倍）** | **154,618 バイト（約 2.63 倍）** |
| 参考： 本編 C# 版 v2.2.2 | 289,967,656 バイト | — |

Rust 版は Delphi 版の約2倍。どちらも本編の約1/1000という桁は同じ。
差の主因は Rust 標準ライブラリ（文字列整形・コレクション等の機械一式）が
静的に入ること。リリースプロファイルは小ささ優先に振ってある
（opt-level="z"、LTO、codegen-units=1、strip、panic="abort"）。

## ツールチェーン

- Rust **1.100.0**（rustc 0ef1bc386 2026-11-09 / cargo 1.100.0）
  - rustup 公式インストーラ（https://sh.rustup.rs）で `~/.cargo` に導入、stable・minimal プロファイル
  - ターゲット `x86_64-pc-windows-gnu` を追加
- リンカ： **mingw-w64 の `x86_64-w64-mingw32-gcc`（GCC 13-win32、apt の gcc-mingw-w64-x86-64 13.2.0-6ubuntu1+26.1）**
  ＝ rustc の windows-gnu 既定ルート（下記「リンク方式の経緯」参照）
- GUI： `windows-sys` **0.59.0**（crates.io 公式。生の Win32 API バインディングのみ、フレームワークなし）
- ソース構成：
  - `src/main.rs` — 入口（Windows では GUI 起動、それ以外では案内のみ）
  - `src/win.rs` — 生 Win32 GUI 本体（Delphi 版 `Mp3SilenceMini.pas` の移植）
  - `src/parse.rs` — silencedetect ログ解析（Delphi 版 `SilenceParse.inc` の移植。単体テスト付き）

ビルドコマンド：
```
cargo build --release --target x86_64-pc-windows-gnu
cargo test            # パーサの単体テスト（ホスト側で実行）
```

### リンク方式の経緯（正直な記録）

1. 最初に apt で mingw を入れようとしたが、この VM の基盤側プロセスが
   apt のロックを長時間保持しており、その時点ではインストールできなかった。
2. 先にフォールバック（rust-lld ＋ link-self-contained=yes）を試した。
   コンパイルは通ったがリンクで失敗： 現在の Rust の self-contained
   フォルダには CRT オブジェクト（crt2.o 等）しか同梱されておらず、
   kernel32/msvcrt 等のインポートライブラリは含まれないため、
   標準ライブラリを使う exe はこの方式だけではリンクできないことを確認した。
3. その後 apt のロックが解放されたため、予定どおり mingw-w64 を
   インストールし、既定ルートでビルド成功（本成果物）。
   `.cargo/config.toml` の rust-lld 上書きは撤去済み。

## アプリ仕様（Delphi 版と完全一致）

- タイトル： 「MP3無音検出ミニ（Rust実験版）」、ウィンドウ 680×460
- 「参照」ボタン： 開くダイアログ（*.mp3; *.wav）
- 「無音を検出」： `ffmpeg -nostdin -i <file> -af silencedetect=noise=-45dB:d=1.0 -f null -`
  を実行（しきい値 −45 dB / 最小 1.0 秒で固定）。区間を
  `N)  開始 ～ 終了 秒  （長さ X 秒）` の形でリスト表示し、
  ステータスに `無音区間： N 件 ／ 無音の合計： X.XXX 秒` を表示
- ffmpeg 探索順： ① exe と同じフォルダ ② `%LocalAppData%\Mp3SilenceCutter\bin\ffmpeg.exe`
  （本編の展開先） ③ PATH（SearchPathW）。無い場合は日本語で案内を表示
- Delphi 版の修正履歴で確立した堅牢化もすべて移植済み：
  - SearchPathW の最終引数には変数のアドレスを渡す（第1回修正の根本原因）
  - CreateProcessW は lpApplicationName とコマンドライン先頭の二重指定、
    コマンドラインは自前の可変バッファ、エラーコードは直後に捕捉（第2回修正）
  - 起動直前に「使う文字列そのもの」の実在チェック＋専用メッセージ（第3回修正）
  - stdin は NUL デバイス、開けなければパイプにフォールバック
  - 起動失敗時はエラーコード＋Windows のシステムメッセージ＋使用パスを表示
  - 検出中は「検出中です… ffmpeg: ＜パス＞」を先に表示
  - どの経路でもボタンは必ず再有効化される
- アニメーションなし

## 検証

- **パーサ単体テスト（`cargo test`、ホスト Linux で実行）： 3/3 PASS**
  - 合成ログ： Delphi 版 TestParse と同一内容・同一期待値
    → 2 区間（1.500–3.250 / 10.000–12.500）、末尾の対にならない silence_start は無視
  - 実ログ： Delphi 版と同じ `ffmpeg_log.txt`（無音位置が既知の WAV を実 ffmpeg で
    処理した stderr）→ **2.000–5.000 / 7.000–9.000** を抽出、Delphi 版と同一結果
  - 秒表示フォーマット（0.000 形式）も一致を確認
- **PE / ネイティブ確認**： `file` で PE32+ executable (GUI) x86-64。
  インポートは KERNEL32 / kernel32 / user32 / gdi32 / comdlg32 / msvcrt / ntdll /
  api-ms-win-core-synch-l1-2-0 のみ（すべて Windows 標準 DLL）。
  **mscoree 参照 0 件＝.NET ではない**ことを objdump で確認。
- **日本語文字列**： タイトル・ボタン・ステータス文言・ffmpeg 引数が exe 内に
  格納されていることをバイトレベルで確認。なお格納形式は Delphi 版と違い
  UTF-8 で、実行時に `encode_utf16` で UTF-16 へ変換して API に渡す
  （ロケール変換を経由しないため文字化け経路なし）。

## 正直な制限

- この VM には Windows が無いため、**実機 Windows での GUI 起動確認は未実施**
  （PE構造・依存DLL・文字列格納・パーサまでは検証済み）。最終確認は実機でお願いします。
  実機テストの観点は Delphi 版と同じです：
  ① 起動してタイトル「MP3無音検出ミニ（Rust実験版）」が出るか
  ② 「参照」で MP3/WAV を選べるか
  ③ 本編と同じフォルダ（`%LocalAppData%\Mp3SilenceCutter\bin` の ffmpeg が
     見つかる場所）で「無音を検出」を押して、Delphi 版と同じ件数・合計秒数が
     出るか（同じファイルなら数値は完全一致するはずです）
- ファイル末尾まで続く無音（silence_end が出ない区間）は一覧に出ません
  （開始/終了が対になった区間のみ。Delphi 版と同じ仕様）。
- 検出パラメータは実験用に固定（-45 dB / 1.0 秒）。本編のような調整・波形・
  カット保存・AI判定はありません。
