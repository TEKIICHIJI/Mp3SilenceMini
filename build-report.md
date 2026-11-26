# Mp3SilenceMini（Delphi実験版）ビルドレポート

目的： Delphi（Object Pascal）＋ネイティブコードで「小さな実験版」を作り、本編 C# 版とサイズを比べる実験。

## 成果物

| 項目 | 値 |
|---|---|
| exe | `~/workspace/your_files/Mp3SilenceMini-Delphi.exe` |
| exe サイズ | **140,288 バイト**（約137 KB） |
| exe SHA256 | `d19ddc5d2a3ca116267c7de963752d1ef25a4f33edfeab58add142ac86acb9c8` |
| zip | `~/workspace/your_files/Mp3SilenceMini-Delphi.zip` |
| zip サイズ | **57,485 バイト** |
| zip SHA256 | `333e6ad55322233350640838de9b2a07c9585ab21d44dbe6ef7b83c7d01dcff2` |
| 参考： 本編 v1.4 exe | 287,125,032 バイト → 実験版は約 **1/2000**（ただし本編は ffmpeg・ONNX・.NET ランタイム同梱分が大半で、実験版は ffmpeg を外部から借りる設計。その差を引いても、ネイティブ単体コード自体は極小） |

## ツールチェーン

- Free Pascal **3.2.2**（Delphi互換 Object Pascal）
  - ネイティブコンパイラ： Ubuntu noble の `fp-compiler-3.2.2`（apt、gtk依存を避けるため最小構成で導入）
  - win64 クロスコンパイラ＋RTL： FPC 公式ソース（GitHub FPCSource `release_3_2_2`）から `make crossinstall CPU_TARGET=x86_64 OS_TARGET=win64` で自前ビルド → `/opt/fpcross`
  - リンカ等： `binutils-mingw-w64-x86-64`（x86_64-w64-mingw32-as / ld）
- 実装方式： raw Win32 API（Unicode の W 系関数を直接使用）、LCL 不使用
- ソース： `Mp3SilenceMini.pas`（本体）＋ `SilenceParse.inc`（silencedetect ログ解析、本体とテストで共有）＋ `TestParse.pas`（テスト）

ビルドコマンド（要点）：
```
ppcrossx64 -n -Twin64 -Px86_64 -O2 -Xs \
  -Fu<units>/rtl -Fu<units>/rtl-objpas -Fu<units>/winunits-base \
  -o Mp3SilenceMini.exe Mp3SilenceMini.pas
```
※ ppcrossx64 のデフォルト対象は linux 表示になるため、`-Twin64 -Px86_64` の明示が必須だった（ハマりどころとして記録）。

## 検証

- パイプライン証明： 最初に win64 GUI hello-world をクロスコンパイルし、`PE32+ executable (GUI) x86-64` を確認してから本実装に入った。
- PE / ネイティブ確認： `file` で PE32+ GUI x86-64。インポートは kernel32 / user32 / gdi32 / comdlg32 / oleaut32 のみ、**mscoree 参照 0 件＝.NET ではない**ことを objdump で確認。
- 日本語文字列： タイトル等のリテラルが UTF-16LE で exe 内に格納されていることをバイトレベルで確認（文字化け経路なし）。
- パーサテスト（`TestParse`、本体と同一の include コードで実行）：
  - 合成ログ： 期待どおり 2 区間（1.500–3.250 / 10.000–12.500）を抽出、末尾の対にならない silence_start は無視 → **PASS**
  - 実ログ： 無音位置が既知の WAV（トーン2s→無音3s→トーン2s→無音2s→トーン1s、`test_silence.wav`）を実 ffmpeg（silencedetect noise=-45dB:d=1.0）で処理した stderr（`ffmpeg_log.txt`）を解析 → **2.000–5.000 / 7.000–9.000** を抽出、ffmpeg 自身の報告（～5.000023 / ～9.000023）と一致 → **PASS**

## アプリ仕様（実験用に固定）

- 参照ボタン： 開くダイアログ（*.mp3; *.wav）
- 「無音を検出」： `ffmpeg -nostdin -i <file> -af silencedetect=noise=-45dB:d=1.0 -f null -` を実行し、区間をリスト表示＋件数と合計秒数をステータス表示（引数の形は本編 C# 版に一致。第2回修正で `-f null NUL` から変更）
- ffmpeg 探索順： ① exe と同じフォルダ ② `%LocalAppData%\Mp3SilenceCutter\bin\ffmpeg.exe`（本編の展開先） ③ PATH。無い場合は日本語で案内を表示
- アニメーションなし

## 修正履歴： 2026-11-22 「無音を検出」でクラッシュする不具合

実機（Windows 11）報告： 起動・参照ダイアログ・ファイル選択は正常だが、「無音を検出」を押すと
`EAccessViolation: Access violation`（未処理例外ダイアログ）で落ちる。ステータス表示が
「検出中です…」に変わる前に落ちていたことから、異常箇所は `FindFFmpeg` 内と特定した。

### 根本原因

`FindFFmpeg` のステップ③（PATH 検索）の `SearchPathW` 呼び出し：

```pascal
PartPtr : PWideChar;  { 未初期化 }
N := SearchPathW(nil, 'ffmpeg.exe', nil, Length(Buf), @Buf[0], PartPtr);  { ← バグ }
```

`SearchPathW` の最終引数は `LPWSTR *lpPart`（PWideChar 変数へのポインタ）で、API は
「見つけたファイル名部分のアドレス」をその先に**書き込む**。ところがコードは変数の
**アドレス（@PartPtr）ではなく、未初期化の変数の値（ゴミアドレス）そのもの**を渡していた。
ステップ①②で ffmpeg が見つからずステップ③に到達すると、API が無効なアドレスへ
書き込みを行い、システム DLL 内でアクセス違反が発生する。これがクラッシュの原因。

なお他の容疑箇所は精査の結果いずれも正常だった：
- `CreateProcessW` のコマンドラインは専用の可変バッファ（動的配列）にコピーして渡していた（問題なし）
- `STARTUPINFOW.cb` 設定、パイプの書き込み側ハンドルの親側クローズ、読み取り前の有効性確認も正常
- ffmpeg 探索ステップ②の LOCALAPPDATA 展開も安全（失敗時は空文字を返すだけ）

### 変更内容

1. **根本修正**： `PartPtr := nil;` で初期化し、`@PartPtr`（変数のアドレス）を渡すよう変更。
2. **安全網**： `DoDetect` 全体を try/except で包み、あらゆる失敗時にステータス表示＋日本語の
   メッセージボックスで知らせるようにした（未処理例外ダイアログで落ちることはなくなる）。
   ボタンは finally/except の両経路で必ず再有効化する。
3. **予防的堅牢化**： `FindFFmpeg` 全体も例外ガード（失敗時は「見つからない」として処理継続）。
   `RunCapture` では `NUL` が開けなかった場合に無効ハンドルを `CreateProcess` へ渡さないよう変更。

### 修正版の成果物

| 項目 | 値 |
|---|---|
| exe サイズ | **141,312 バイト**（修正前 140,288 → +1,024 バイト、安全網の分） |
| exe SHA256 | `efe067586ac948fe71e4f6451e4dcdd2bc05a7384c06473cc5667f6ab854e557` |
| zip サイズ | **57,968 バイト** |
| zip SHA256 | `b002c292e94d98884d6baa1ad7f80d5876c2d2cfc6a501d2898b3ac90a1876ab` |

### 修正版の検証

- 同じ Free Pascal 3.2.2 win64 クロス toolchain で再ビルド（ソースツリー内の ppcrossx64＋
  in-tree units を使用。環境リセットで消えていた mingw binutils のみ apt で再導入）
- パーサテスト再実行： 合成ログ PASS（1.500–3.250 / 10.000–12.500、末尾の対なし start は無視）、
  実ログ PASS（**2.000–5.000 / 7.000–9.000**）— 修正前と同一結果
- PE 確認： PE32+ (GUI) x86-64、インポートは kernel32 / user32 / gdi32 / comdlg32 / oleaut32 のみ、
  mscoree 参照 0 件＝完全ネイティブを維持
- 新しい安全網の日本語文字列が exe 内に UTF-16LE で格納されていることをバイトレベルで確認
- 実機 Windows での修正版の動作確認は未実施（次回の実機テストで確認をお願いします）

## 修正履歴： 2026-11-22（第2回）ffmpeg が「起動できない」不具合

実機での再テスト報告：
1. 第1回修正は有効だった。ffmpeg が無い状態で「無音を検出」を押すと、クラッシュせず
   丁寧なダイアログ（「ffmpeg.exe が見つかりませんでした」＋3か所の案内）が出ることを確認。
2. その後、実験版 exe と同じフォルダに ffmpeg.exe を配置（エクスプローラのスクリーン
   ショットで確認： 107,999 KB ＝ 本編同梱 ffmpeg と同一の 110,590,976 バイト。本編が
   同じ PC で 743 件の処理に使った正常品）。探索ステップ①で発見されるようになったが、
   今度はステータス表示が「ffmpeg を起動できませんでした。」になり、`RunCapture` 内の
   `CreateProcessW` が失敗した。

### 原因の分析（起動コードを精査）

第1回時点の起動コードには、古典的な落とし穴が2つ残っていた：

1. **`lpApplicationName = nil` ＋コマンドライン先頭の引用符付きパス**：
   実行ファイルの場所をコマンドライン文字列の解析だけに頼る形で、引用符付き先頭
   トークンの解釈に失敗すると起動できない。→ 実行ファイルパスを `lpApplicationName`
   にも明示的に渡し、コマンドライン先頭にも引用符付きで置く二重指定に変更（解析に
   依存しない形）。
2. **stdin ハンドルのフォールバックが `0`**： NUL デバイスが開けなかった場合に
   `si.hStdInput := 0` としており、`STARTF_USESTDHANDLES` 使用時に無効な std
   ハンドルを渡すと `CreateProcess` は `ERROR_INVALID_HANDLE` で失敗する。→ NUL が
   開けない場合はパイプにフォールバック（書き込み側は親が保持し、起動直後に閉じる
   ので子は EOF を見る）し、無効なハンドルは絶対に渡さない形に変更。

その他の点は正常だった： コマンドラインは専用の可変バッファにコピーして渡していた、
`STARTUPINFOW.cb` 設定済み、stderr パイプの読み取り側は子に継承させない設定済み、
書き込み側は起動後に親側でクローズ。本編（C#）の起動方法とも比較し、引数の形を
本編と同一（`-f null -`）に揃えた。

### 変更内容（`Mp3SilenceMini.pas` のみ）

1. **起動方法の堅牢化**（上記1・2）。`RunCapture` の引数を「コマンドライン全体」から
   「実行ファイルパス＋引数」に分離し、内部で引用符付きコマンドラインを組み立てる。
   `GetLastError` は失敗した API の直後に（他の API を呼ぶ前に）捕捉する。
2. **診断表示の強化**： 起動失敗時はエラーコードと Windows のシステムメッセージ
   （`FormatMessageW`）を含む表示に変更 — 例：「ffmpeg を起動できませんでした
   （エラー 5：アクセスが拒否されました）」。ダイアログには使用した ffmpeg の
   フルパスも併記。さらに、実行直前のステータス表示で使用する ffmpeg のパスを
   先に示す（`検出中です… ffmpeg: <パス>`）ようにし、次のスクリーンショットだけで
   状況が分かるようにした。

### 第2回修正版の成果物

| 項目 | 値 |
|---|---|
| exe サイズ | **142,848 バイト**（第1回修正版 141,312 → +1,536 バイト、診断表示等の分） |
| exe SHA256 | `1bd48c6d366a5646d66fb5f68a7891e3aedf2990ec8b113f4ad784ba23103930` |
| zip サイズ | **58,388 バイト** |
| zip SHA256 | `5c819cc35358f13fd08782b5f0aaf6ea00c02023fa952bcbced3061b6d2c247f` |

### 第2回修正版の検証

- 同じ Free Pascal 3.2.2 win64 クロス toolchain で再ビルド。なお、未修正のソースを
  先にビルドして従来と同一の 141,312 バイトになることを確認してから修正版を
  ビルドした（toolchain の同一性を担保）。
- パーサテスト再実行： 合成ログ PASS（1.500–3.250 / 10.000–12.500、末尾の対なし
  start は無視）、実ログ PASS（**2.000–5.000 / 7.000–9.000**）— 従来と同一結果。
- PE 確認： PE32+ (GUI) x86-64、インポートは kernel32 / user32 / gdi32 / comdlg32 /
  oleaut32 のみ、mscoree 参照 0 件＝完全ネイティブを維持。
- 新しい診断メッセージ等の日本語文字列が exe 内に UTF-16LE で格納されていることを
  バイトレベルで確認。
- 実機 Windows での動作確認は未実施（次回の実機テストで確認をお願いします）。
  万一まだ起動できない場合でも、今回はエラーコードと対象パスが画面に出るため、
  そのスクリーンショットだけで原因を特定できます。

## 修正履歴： 2026-11-22（第3回）ffmpeg のパスが「I」の1文字になっていた不具合

実機での再テスト報告： 第2回で追加した診断表示が完璧に機能し、原因が画面だけで特定できた。
ステータス表示は「ffmpeg を起動できませんでした（エラー 2：指定されたファイルが見つかりません）」、
ダイアログのパス表示は「パス： I」— **CreateProcess に渡された ffmpeg のパスが
「I」の1文字だけ**になっていた（実験版 exe は I: ドライブ上にあるため、本来は
「I:\...\ffmpeg.exe」のような文字列のはず）。

### 根本原因（実証済み）

`FindFFmpeg` の3ステップすべてで使っていた

```pascal
SetString(Dir, @Buf[0], N);   { Buf は array of WideChar }
```

が元凶。FPC のデフォルト（型付きアドレス検査オフ）では `@Buf[0]` は**無型ポインタ**になり、
`SetString` のオーバーロード解決が **PAnsiChar 版**を選んでしまう。その結果、UTF-16 の
バイト列が「1バイトずつ WideChar に拡張」されてコピーされ、文字列は
'I', #0, ':', #0, '\', #0, … という内容になる。Pascal 的には長さのある文字列だが、
PWideChar 経由で見る C API からは最初の #0 で終わるため **「I」としか見えない**。

この機構で全症状が説明できる：
- ステップ①②： 候補パスの C としての姿が「I」「C」になるため、実在チェック
  （`GetFileAttributesW`）が失敗し、ffmpeg が横にあっても「見つからない」扱いだった
- ステップ③： 実在チェックを通さずそのまま返すため、化けたパスが返り、
  `CreateProcessW` がエラー 2（ファイルが見つかりません）になった。
  **第1回修正版（build 2）の「起動できませんでした」も同じ原因**で、当時は診断表示が
  無かったため見えなかっただけである
- 対照的に `WinErrorText` の `SetString(S, Buf, N)` は PWideChar **変数**を渡すため
  正しいオーバーロードが選ばれており、第2回修正版で Windows の日本語エラーメッセージが
  全文表示されていたこととも一致する

検証方法： 本番と同一の操作（同一ディレクティブ・同一コード）をネイティブ FPC で
再現するテストを作成し、文字列の長さと全文字コードをダンプして確認した。
`SetString(..., @Buf[0], ...)` は ords=[73,0,58,0,92,0,...]（＝バイト拡張）になり、
PWideChar 変数渡し／明示キャスト渡しでは ords=[73,58,92,...]（＝正常）になることを実証。
ついでに監査した他の変換はすべて正常だった：
`WinErrorText` の SetString（PWideChar 変数）、`RunCapture` の
`SetString(Output, PAnsiChar(@data[0]), ...)`（明示キャスト＋AnsiString 受けでバイト
コピーが意図どおり）、`DoDetect` の `WideString(PathBuf)` キャスト（実測で全文正常）、
`DoBrowse` のバッファ直接渡し。

### 変更内容（`Mp3SilenceMini.pas` のみ）

1. **根本修正**： バッファ→WideString 変換を、オーバーロード解決に依存しない
   専用ヘルパー `WideFromBuf`（`SetLength`＋`Move` で WideChar をそのままコピー）に
   一本化し、`FindFFmpeg` の3ステップと `WinErrorText` をすべて置き換え。
   パスは最初から最後まで WideString で受け渡す。
2. **暗黙の ANSI 変換の排除**： API へ渡す名前文字列（`'ffmpeg.exe'`、
   `'LOCALAPPDATA'`、`'NUL'`）も WideString 変数を経由して `PWideChar(...)` で
   明示的に渡すよう変更。`DoDetect` の入力パスも `GetWindowTextW` の戻り値（文字数）と
   `WideFromBuf` で構築するよう変更（キャスト自体は正常だったが一本化）。
3. **起動前の実在チェック追加**： `FindFFmpeg` がパスを返した後、`CreateProcess` の前に、
   その文字列そのものが実在ファイルか（`FileExistsW`＝`GetFileAttributesW`）を確認する。
   ダメな場合は起動エラーとは別の専用メッセージ
   「ffmpegの場所は見つかりましたが、ファイルにアクセスできません：＜パス＞」を
   ステータスとダイアログに表示する。
4. 第2回の診断表示（エラーコード＋システムメッセージ＋パス）と try/except の安全網は
   そのまま維持。

### 第3回修正版の成果物

| 項目 | 値 |
|---|---|
| exe サイズ | **143,360 バイト**（第2回修正版 142,848 → +512 バイト、実在チェック等の分） |
| exe SHA256 | `3eb16b99aa5e24551a0a1f466f8602765fbcdfe1ac822497718e90111ab44204` |
| zip サイズ | **58,720 バイト** |
| zip SHA256 | `1835760495e81609504645dc94f838282d0b9193b3607b520fc7fbff34fad8a4` |

### 第3回修正版の検証

- 同じ Free Pascal 3.2.2 win64 クロス toolchain（ソースツリー内の ppcrossx64＋in-tree
  units）で再ビルド。ネイティブ FPC（apt の fp-compiler-3.2.2）は文字列再現テスト用に使用。
- パーサテスト再実行： 合成ログ PASS（1.500–3.250 / 10.000–12.500、末尾の対なし
  start は無視）、実ログ PASS（**2.000–5.000 / 7.000–9.000**）— 従来と同一結果。
- PE 確認： PE32+ (GUI) x86-64、インポートは kernel32 / user32 / gdi32 / comdlg32 /
  oleaut32 のみ、mscoree 参照 0 件＝完全ネイティブを維持。
- 新しい専用メッセージ等の日本語文字列が exe 内に UTF-16LE で格納されていることを
  バイトレベルで確認。
- ソース監査： WideString パスに対する `SetString(..., @配列[0], ...)` 形式は全廃
  （残る SetString は RunCapture の AnsiString 用・明示 PAnsiChar キャスト付きのみ）。
- 実機 Windows での動作確認は未実施（次回の実機テストで確認をお願いします）。
  今回はパス文字列の化けそのものを断ったため、ffmpeg が横にあれば検出まで到達する
  はずです。万一パスの実在チェックに引っかかる場合も、専用メッセージでパス全文が
  画面に出ます。

## 正直な制限

- この VM には Windows が無いため、**実機 Windows での GUI 起動確認は未実施**（PE構造・依存DLL・文字列格納までは検証済み）。最終確認は実機でお願いします。
- ファイル末尾まで続く無音（silence_end が出ない区間）は一覧に出ません（開始/終了が対になった区間のみ）。
- 検出パラメータは実験用に固定（-45 dB / 1.0 秒）。本編のような調整・波形・カット保存・AI判定はありません。
