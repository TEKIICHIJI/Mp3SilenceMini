# Mp3SilenceMini

MP3/WAVの無音区間を検出して一覧表示するだけの、小さな実験版アプリです（Windows用）。
Free Pascal 3.2.2で作った完全ネイティブのexeで、サイズは約140KB。カットや保存はしません。

A tiny experimental Windows tool that detects and lists silent segments in MP3/WAV files. Native Free Pascal, ~140 KB. Detection only — no cutting.

**Note for non-Japanese users:** The app UI is in Japanese only. On a Windows PC without Japanese language support, the UI text may display garbled (mojibake). The detection itself works the same. The app also needs ffmpeg.exe, which is not included — see "ffmpegが必要です" below.

## 使い方

1. 音声ファイルを1つ選ぶ
2. 「無音を検出」を押す
3. 無音区間（開始〜終了）と件数・合計秒数が表示されます

検出条件は固定です：-45dB / 1.0秒以上。

## ffmpegが必要です（同梱していません）

このアプリ自体はffmpegを含みません。次の順番で探します。

1. exeと同じフォルダの `ffmpeg.exe`
2. `%LocalAppData%\Mp3SilenceCutter\bin\ffmpeg.exe`
3. PATHの通った `ffmpeg.exe`

見つからない場合は日本語で案内が出ます。

## ダウンロード

Releasesの **v0.1.0** からzipをダウンロードしてください。中身はexe 1本だけです。

- exe：143,360バイト、SHA256 `3eb16b99aa5e24551a0a1f466f8602765fbcdfe1ac822497718e90111ab44204`
- zip：58,720バイト、SHA256 `1835760495e81609504645dc94f838282d0b9193b3607b520fc7fbff34fad8a4`

## これは何の実験か

本編のMP3カッター（C#版）と比べ、ネイティブ版がどれだけ小さく・軽くなるかの実験です。
本編の安全弁つき一括処理・AI判定・ガラスUIは入っていません。

詳しい記録は `build-report.md` を見てください。

## 実績

Windows 11の実機で検出を確認済み：247件・合計758.478秒。

## ライセンス

MIT License — Copyright (c) 2026 Fuwatoro Box (TEKIICHIJI)
詳細は `LICENSE` を見てください。

---

## Rust版

同じ無音検出ミニを Rust で書き直したものです。Delphi版と同じ仕様・同じUI文言です。

- 生の Win32 API を `windows-sys` 経由で直接呼び出しています（フレームワーク不使用）。
- 外部 ffmpeg を使います（同梱していません）。
- 検出条件は固定です：-45dB / 1.0秒以上（Delphi版と同じ）。

### サイズ比較

- Delphi版 exe：143,360バイト
- Rust版 exe：292,864バイト

### 検出結果の一致

Windows 11の実機で、同じファイルで両版の検出結果が一致することを確認済みです：295件・合計1,284.171秒。

### ビルド方法

```sh
cargo build --release --target x86_64-pc-windows-gnu
```

ソースは `rust/` フォルダにあります。詳しい記録は `rust/build-report.md` を見てください。

### ダウンロード

Releasesの **v0.2.0** から Rust版のzipをダウンロードしてください。中身はexe 1本だけです。
