import { observer } from "mobx-react-lite";
import { useEffect, useRef, useState } from "react";
import type { RootStore } from "../../app/RootStore.js";
import {
  type AnnotationSeverity,
  type AnnotationTag,
  ANNOTATION_SEVERITY_LABELS,
  ANNOTATION_TAG_LABELS,
} from "../../domain/debuggerModels.js";

type Props = { store: RootStore };

const TAGS: AnnotationTag[] = [
  "wrong_cue", "missing_cue", "wrong_ui", "missing_ui",
  "wrong_timing", "gps_issue", "reroute_issue", "other",
];

const SEVERITIES: AnnotationSeverity[] = ["bug", "improvement", "note"];

export const DebuggerAnnotationForm = observer(({ store }: Props) => {
  const dStore = store.debuggerStore;
  const [tag, setTag] = useState<AnnotationTag>("other");
  const [severity, setSeverity] = useState<AnnotationSeverity>("bug");
  const [startOffset, setStartOffset] = useState(0);
  const [endOffset, setEndOffset] = useState(5000);
  const [note, setNote] = useState("");
  const formRef = useRef<HTMLDivElement>(null);

  // Keep form visible above the on-screen keyboard via visualViewport API
  useEffect(() => {
    const viewport = window.visualViewport;
    if (!viewport) return;

    const handleResize = () => {
      if (!formRef.current) return;
      const visibleHeight = viewport.height;
      if (visibleHeight < window.innerHeight * 0.8) {
        formRef.current.style.setProperty("margin-bottom", `${window.innerHeight - visibleHeight + 8}px`);
      } else {
        formRef.current.style.removeProperty("margin-bottom");
      }
    };

    viewport.addEventListener("resize", handleResize);
    viewport.addEventListener("scroll", handleResize);
    return () => {
      viewport.removeEventListener("resize", handleResize);
      viewport.removeEventListener("scroll", handleResize);
    };
  }, []);

  if (dStore.pendingAnnotationTimeMs === null || !dStore.session) return null;

  const sessionStart = dStore.session.diagSession.events[0]?.timestampMs ?? dStore.session.diagSession.createdAtMs;
  const relMs = Math.max(0, dStore.pendingAnnotationTimeMs - sessionStart);

  const handleSave = () => {
    dStore.addAnnotation(
      tag,
      severity,
      [relMs + startOffset, relMs + endOffset],
      note,
    );
    resetForm();
  };

  const handleCancel = () => {
    dStore.cancelAnnotationForm();
    resetForm();
  };

  const resetForm = () => {
    setTag("other");
    setSeverity("bug");
    setStartOffset(0);
    setEndOffset(5000);
    setNote("");
  };

  const formatMs = (ms: number) => {
    const sec = Math.floor(ms / 1000);
    return `${sec}s`;
  };

  return (
    <div ref={formRef} className="debugger-annotation-form">
      <div className="debugger-annotation-form__header">New Annotation</div>
      <div className="debugger-annotation-form__time">
        At {formatMs(relMs)} (range: {formatMs(relMs + startOffset)} – {formatMs(relMs + endOffset)})
      </div>

      <label className="debugger-annotation-form__label">
        Tag
        <select value={tag} onChange={(e) => setTag(e.target.value as AnnotationTag)}>
          {TAGS.map((t) => (
            <option key={t} value={t}>{ANNOTATION_TAG_LABELS[t]}</option>
          ))}
        </select>
      </label>

      <label className="debugger-annotation-form__label">
        Severity
        <select value={severity} onChange={(e) => setSeverity(e.target.value as AnnotationSeverity)}>
          {SEVERITIES.map((s) => (
            <option key={s} value={s}>{ANNOTATION_SEVERITY_LABELS[s]}</option>
          ))}
        </select>
      </label>

      <div className="debugger-annotation-form__range">
        <label className="debugger-annotation-form__label">
          Start offset
          <input
            type="range"
            min={-10000}
            max={10000}
            step={100}
            value={startOffset}
            onChange={(e) => setStartOffset(Number(e.target.value))}
          />
          <span>{startOffset > 0 ? "+" : ""}{formatMs(startOffset)}</span>
        </label>
        <label className="debugger-annotation-form__label">
          End offset
          <input
            type="range"
            min={-10000}
            max={30000}
            step={100}
            value={endOffset}
            onChange={(e) => setEndOffset(Number(e.target.value))}
          />
          <span>{endOffset > 0 ? "+" : ""}{formatMs(endOffset)}</span>
        </label>
      </div>

      <label className="debugger-annotation-form__label">
        Note
        <textarea
          value={note}
          onChange={(e) => setNote(e.target.value)}
          placeholder="Describe the issue..."
          rows={3}
        />
      </label>

      {dStore.pendingAnnotationEventIds.length > 0 && (
        <div className="debugger-annotation-form__linked">
          Linked events: {dStore.pendingAnnotationEventIds.join(", ")}
        </div>
      )}

      <div className="debugger-annotation-form__actions">
        <button type="button" className="debugger-annotation-form__btn debugger-annotation-form__btn--save" onClick={handleSave}>
          Save
        </button>
        <button type="button" className="debugger-annotation-form__btn" onClick={handleCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
});
