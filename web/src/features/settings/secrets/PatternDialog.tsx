import { TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import type { PatternKind } from "../../../generated/PatternKind";
import type { PatternScope } from "../../../generated/PatternScope";
import type { RedactionLimits } from "../../../generated/RedactionLimits";
import type { RedactionPattern } from "../../../generated/RedactionPattern";
import type { RedactionPatternTest } from "../../../generated/RedactionPatternTest";
import type { RedactionSettings } from "../../../generated/RedactionSettings";
import { ApiFailure } from "../../../api/client";
import { toFailure } from "../../../lib/useAsync";
import { Dialog } from "../../../ui/Dialog";
import { Segmented } from "../../../ui/Segmented";
import { Button, Field } from "../../../ui/controls";
import { saveSecretPattern, trySecretPattern } from "../api";

const TRY_AFTER_MS = 250;
const KINDS: { value: PatternKind; label: string }[] = [{ value: "text", label: "Text" }, { value: "regex", label: "Regular expression" }];
const SCOPES: { value: PatternScope; label: string }[] = [{ value: "anywhere", label: "Anywhere" }, { value: "environment_values", label: "Environment values" }];

/** The sample with each matched stretch drawn as a mask, the way a terminal block draws a hidden value. */
function Masked({ sample, test }: { sample: string; test: RedactionPatternTest }) {
  const chars = [...sample];
  const parts: React.ReactNode[] = [];
  let at = 0;
  for (const span of test.matches) {
    if (span.start > at) parts.push(chars.slice(at, span.start).join(""));
    parts.push(<span key={span.start} className="secrets-mask" aria-label="hidden value">{"•".repeat(span.end - span.start)}</span>);
    at = span.end;
  }
  if (at < chars.length) parts.push(chars.slice(at).join(""));
  return <span className="mono">{parts}</span>;
}

/**
 * Add or edit a pattern (redact-pattern-add, -error, -broad). Try it on some text runs as the person types; Add
 * pattern is enabled once the pattern matches the sample; a pattern that would hide nearly everything says so with the
 * numbers and is added with Add anyway. The service checks again when it saves.
 */
export function PatternDialog({
  pattern,
  limits,
  onSaved,
  onClose,
}: {
  pattern: RedactionPattern | null;
  limits: RedactionLimits;
  onSaved(settings: RedactionSettings): void;
  onClose(): void;
}) {
  const [name, setName] = useState(pattern?.name ?? "");
  const [text, setText] = useState(pattern?.pattern ?? "");
  const [kind, setKind] = useState<PatternKind>(pattern?.kind ?? "regex");
  const [scope, setScope] = useState<PatternScope>(pattern?.applies_to ?? "anywhere");
  const [sample, setSample] = useState("");
  const [test, setTest] = useState<RedactionPatternTest | null>(null);
  const [busy, setBusy] = useState(false);
  const [refused, setRefused] = useState<string | null>(null);

  useEffect(() => {
    setTest(null);
    setRefused(null);
    if (!text.trim()) return;
    const controller = new AbortController();
    const timer = setTimeout(() => {
      trySecretPattern({ pattern: text, kind, applies_to: scope, sample }, controller.signal)
        .then(setTest)
        .catch(() => undefined);
    }, TRY_AFTER_MS);
    return () => { clearTimeout(timer); controller.abort(); };
  }, [text, kind, scope, sample]);

  const problem = test?.problem ?? null;
  const matched = test && !problem ? test.matches.length : 0;
  const broad = Boolean(test && !problem && test.broad);
  const ready = name.trim() !== "" && test !== null && !problem && matched > 0;

  const save = async (acceptBroad: boolean) => {
    setBusy(true);
    setRefused(null);
    try {
      onSaved(await saveSecretPattern(pattern?.id ?? null, { name, pattern: text, kind, applies_to: scope, accept_broad: acceptBroad ? true : undefined }));
      onClose();
    } catch (failure) {
      const error = toFailure(failure);
      setRefused(error instanceof ApiFailure ? error.message : "The pattern could not be saved.");
    } finally {
      setBusy(false);
    }
  };

  const title = pattern ? "Edit pattern" : "Add a pattern";
  return (
    <Dialog
      title={title}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>Cancel</Button>
          <Button tone="primary" busy={busy} disabled={!ready} onClick={() => void save(broad)}>
            {broad ? "Add anyway" : pattern ? "Save" : "Add pattern"}
          </Button>
        </>
      }
    >
      <div className="form">
        <Field label="Name">
          {(props) => <input {...props} data-dialog-focus type="text" value={name} maxLength={limits.max_name_chars} onChange={(event) => setName(event.target.value)} />}
        </Field>
        <Field label="Pattern" error={problem?.message ?? null} hint={kind === "text" ? "Plain text. * stands for any run of characters that are not spaces." : undefined}>
          {(props) => (
            <input {...props} className="mono" type="text" value={text} maxLength={limits.max_pattern_chars} spellCheck={false} autoCapitalize="off" autoCorrect="off" onChange={(event) => setText(event.target.value)} />
          )}
        </Field>
        <div className="secrets-two">
          <div className="field">
            <span id="secrets-kind">Pattern type</span>
            <Segmented<PatternKind> label="Pattern type" value={kind} onChange={(next) => setKind(next)} options={KINDS} />
          </div>
          <div className="field">
            <span id="secrets-scope">Applies to</span>
            <Segmented<PatternScope> label="Applies to" value={scope} onChange={(next) => setScope(next)} options={SCOPES} />
          </div>
        </div>
        <Field label="Try it on some text">
          {(props) => (
            <input {...props} className="mono" type="text" value={sample} maxLength={limits.max_sample_chars} spellCheck={false} placeholder="Paste a line to see what would be hidden" onChange={(event) => setSample(event.target.value)} />
          )}
        </Field>
        <div className="secrets-result" role="status" aria-live="polite">
          {problem || !text.trim() ? (
            <><b>Nothing to test yet</b>{problem ? "Fix the pattern to see what it would hide." : "Enter a pattern to see what it would hide."}</>
          ) : !test ? (
            <b>Trying the pattern…</b>
          ) : broad ? (
            <><b>Matched {test.lines_matched} of {test.lines_total} lines</b>Of the ordinary text Orbyn tried it on</>
          ) : matched > 0 ? (
            <><b>Matched {matched} {matched === 1 ? "value" : "values"}</b><Masked sample={sample} test={test} /></>
          ) : (
            <><b>No match</b>{sample ? "Nothing in this text would be hidden." : "Add some text to try the pattern on."}</>
          )}
        </div>
        {broad ? (
          <div className="secrets-warn" role="note">
            <TriangleAlert size={15} aria-hidden />
            <div><b>This pattern is very broad.</b> It would hide {test?.lines_matched} of {test?.lines_total} lines of ordinary text. Check the pattern, or add it anyway.</div>
          </div>
        ) : null}
        {refused ? <p className="field-error" role="alert">{refused}</p> : null}
      </div>
    </Dialog>
  );
}
