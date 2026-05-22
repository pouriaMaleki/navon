import { observer } from "mobx-react-lite";
import { useEffect, useRef, useState } from "react";
import type { RootStore } from "../../../app/RootStore.js";
import {
  ANNOTATION_SEVERITY_LABELS,
  ANNOTATION_TAG_LABELS,
  type AnnotationSeverity,
  type AnnotationTag,
} from "../../../domain/debuggerModels.js";
import { AnnotationActions } from "./AnnotationActions.js";
import { AnnotationHeader } from "./AnnotationHeader.js";
import { AnnotationLinkedEvents } from "./AnnotationLinkedEvents.js";
import { AnnotationNote } from "./AnnotationNote.js";
import { AnnotationRangeSliders } from "./AnnotationRangeSliders.js";
import { AnnotationSelect } from "./AnnotationSelect.js";
import { AnnotationTime } from "./AnnotationTime.js";
import styles from "./index.module.css";

type Props = { store: RootStore };

const TAGS: AnnotationTag[] = [
  "wrong_cue",
  "missing_cue",
  "wrong_ui",
  "missing_ui",
  "wrong_timing",
  "gps_issue",
  "reroute_issue",
  "other",
];

const SEVERITIES: AnnotationSeverity[] = ["bug", "improvement", "note"];

const TAG_OPTIONS = TAGS.map((t) => ({ value: t, label: ANNOTATION_TAG_LABELS[t] }));
const SEVERITY_OPTIONS = SEVERITIES.map((s) => ({
  value: s,
  label: ANNOTATION_SEVERITY_LABELS[s],
}));

const formatMs = (ms: number) => {
  const sec = Math.floor(ms / 1000);
  return `${sec}s`;
};

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
        formRef.current.style.setProperty(
          "margin-bottom",
          `${window.innerHeight - visibleHeight + 8}px`,
        );
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

  const sessionStart =
    dStore.session.diagSession.events[0]?.timestampMs ?? dStore.session.diagSession.createdAtMs;
  const relMs = Math.max(0, dStore.pendingAnnotationTimeMs - sessionStart);

  const handleSave = () => {
    dStore.addAnnotation(tag, severity, [relMs + startOffset, relMs + endOffset], note);
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

  return (
    <div ref={formRef} className={styles.form}>
      <AnnotationHeader />
      <AnnotationTime
        relMs={relMs}
        startOffset={startOffset}
        endOffset={endOffset}
        formatMs={formatMs}
      />

      <AnnotationSelect
        label="Tag"
        value={tag}
        onChange={(v) => setTag(v as AnnotationTag)}
        options={TAG_OPTIONS}
      />

      <AnnotationSelect
        label="Severity"
        value={severity}
        onChange={(v) => setSeverity(v as AnnotationSeverity)}
        options={SEVERITY_OPTIONS}
      />

      <AnnotationRangeSliders
        startOffset={startOffset}
        endOffset={endOffset}
        onStartChange={setStartOffset}
        onEndChange={setEndOffset}
        formatMs={formatMs}
      />

      <AnnotationNote value={note} onChange={setNote} />

      <AnnotationLinkedEvents eventIds={dStore.pendingAnnotationEventIds} />

      <AnnotationActions onSave={handleSave} onCancel={handleCancel} />
    </div>
  );
});
