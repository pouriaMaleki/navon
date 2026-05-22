import styles from "./AnnotationRangeSliders.module.css";

type Props = {
  startOffset: number;
  endOffset: number;
  onStartChange: (value: number) => void;
  onEndChange: (value: number) => void;
  formatMs: (ms: number) => string;
};

export const AnnotationRangeSliders = ({
  startOffset,
  endOffset,
  onStartChange,
  onEndChange,
  formatMs,
}: Props) => (
  <div className={styles.range}>
    <label className={styles.label}>
      Start offset
      <input
        type="range"
        min={-10000}
        max={10000}
        step={100}
        value={startOffset}
        onChange={(e) => onStartChange(Number(e.target.value))}
      />
      <span>
        {startOffset > 0 ? "+" : ""}
        {formatMs(startOffset)}
      </span>
    </label>
    <label className={styles.label}>
      End offset
      <input
        type="range"
        min={-10000}
        max={30000}
        step={100}
        value={endOffset}
        onChange={(e) => onEndChange(Number(e.target.value))}
      />
      <span>
        {endOffset > 0 ? "+" : ""}
        {formatMs(endOffset)}
      </span>
    </label>
  </div>
);
