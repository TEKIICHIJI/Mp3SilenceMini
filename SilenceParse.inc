{ SilenceParse.inc
  Shared parser for ffmpeg silencedetect output.
  Included by Mp3SilenceMini.pas (the GUI app) and TestParse.pas (console test),
  so the exact same code path is exercised by the automated test.

  ffmpeg silencedetect writes lines like:
    [silencedetect @ 0000...] silence_start: 5.023000
    [silencedetect @ 0000...] silence_end: 9.011000 | silence_duration: 3.988000
  This parser scans the raw log text for the markers "silence_start:" and
  "silence_end:" in order and pairs them up. An unpaired trailing
  silence_start (silence running to end of file) is ignored.
  Requires: StrUtils in the including unit's uses clause (for PosEx).
  All parsing is locale independent (System.Val expects '.' as separator). }

type
  TSilenceRange = record
    StartSec : Double;
    EndSec   : Double;
  end;
  TSilenceRangeArray = array of TSilenceRange;

function TryParseSecondsAt(const S: string; StartPos: Integer; out Value: Double): Boolean;
var
  i, Code : Integer;
  NumStr  : string;
begin
  Result := False;
  Value := 0;
  i := StartPos;
  while (i <= Length(S)) and (S[i] = ' ') do Inc(i);
  NumStr := '';
  while (i <= Length(S)) and (S[i] in ['0'..'9', '.', '-', '+']) do
  begin
    NumStr := NumStr + S[i];
    Inc(i);
  end;
  if NumStr = '' then Exit;
  Val(NumStr, Value, Code);
  Result := (Code = 0);
end;

function ParseSilenceDetectLog(const LogText: string; var Ranges: TSilenceRangeArray): Integer;
const
  MARK_START = 'silence_start:';
  MARK_END   = 'silence_end:';
var
  Offset, pS, pE : Integer;
  v              : Double;
  PendingStart   : Double;
  HasPending     : Boolean;
begin
  SetLength(Ranges, 0);
  Offset := 1;
  HasPending := False;
  PendingStart := 0;
  while True do
  begin
    pS := PosEx(MARK_START, LogText, Offset);
    pE := PosEx(MARK_END,   LogText, Offset);
    if (pS = 0) and (pE = 0) then Break;
    if (pE <> 0) and ((pS = 0) or (pE < pS)) then
    begin
      { an end marker comes first }
      if HasPending and TryParseSecondsAt(LogText, pE + Length(MARK_END), v) then
      begin
        SetLength(Ranges, Length(Ranges) + 1);
        Ranges[High(Ranges)].StartSec := PendingStart;
        Ranges[High(Ranges)].EndSec   := v;
        HasPending := False;
      end;
      Offset := pE + Length(MARK_END);
    end
    else
    begin
      { a start marker comes first }
      if TryParseSecondsAt(LogText, pS + Length(MARK_START), v) then
      begin
        PendingStart := v;
        HasPending := True;
      end;
      Offset := pS + Length(MARK_START);
    end;
  end;
  Result := Length(Ranges);
end;
