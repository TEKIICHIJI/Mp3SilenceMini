program TestParse;
{ Console test for the shared silencedetect log parser (SilenceParse.inc).
  1) Synthetic log with known values (incl. an unpaired trailing start).
  2) Optional: a real ffmpeg stderr log file passed as ParamStr(1);
     prints the parsed ranges for external comparison. }
{$mode delphi}
{$codepage utf8}
uses
  StrUtils, SysUtils;

{$I SilenceParse.inc}

function Near(a, b: Double): Boolean;
begin
  Result := Abs(a - b) < 0.001;
end;

var
  Ranges : TSilenceRangeArray;
  n, i : Integer;
  LogText : string;
  F : TextFile;
  Line : string;
  Failures : Integer;
begin
  Failures := 0;

  { --- synthetic log --- }
  LogText :=
    '[silencedetect @ 000001] silence_start: 1.500000' + #10 +
    'some unrelated ffmpeg chatter' + #10 +
    '[silencedetect @ 000001] silence_end: 3.250000 | silence_duration: 1.750000' + #10 +
    '[silencedetect @ 000001] silence_start: 10.000000' + #10 +
    '[silencedetect @ 000001] silence_end: 12.500000 | silence_duration: 2.500000' + #10 +
    '[silencedetect @ 000001] silence_start: 99.000000' + #10;  { unpaired: must be ignored }
  n := ParseSilenceDetectLog(LogText, Ranges);
  WriteLn('synthetic: count=', n);
  for i := 0 to n - 1 do
    WriteLn('  range ', i + 1, ': ', Ranges[i].StartSec:0:3, ' - ', Ranges[i].EndSec:0:3);
  if n <> 2 then begin WriteLn('FAIL: expected 2 ranges'); Inc(Failures); end
  else
  begin
    if not (Near(Ranges[0].StartSec, 1.5) and Near(Ranges[0].EndSec, 3.25)) then
      begin WriteLn('FAIL: range 1 mismatch'); Inc(Failures); end;
    if not (Near(Ranges[1].StartSec, 10.0) and Near(Ranges[1].EndSec, 12.5)) then
      begin WriteLn('FAIL: range 2 mismatch'); Inc(Failures); end;
  end;
  if Failures = 0 then WriteLn('synthetic: PASS');

  { --- real ffmpeg log from file --- }
  if ParamCount >= 1 then
  begin
    LogText := '';
    AssignFile(F, ParamStr(1));
    Reset(F);
    while not EOF(F) do
    begin
      ReadLn(F, Line);
      LogText := LogText + Line + #10;
    end;
    CloseFile(F);
    n := ParseSilenceDetectLog(LogText, Ranges);
    WriteLn('real log: count=', n);
    for i := 0 to n - 1 do
      WriteLn('  range ', i + 1, ': ', Ranges[i].StartSec:0:3, ' - ', Ranges[i].EndSec:0:3);
  end;

  if Failures > 0 then Halt(1);
end.
