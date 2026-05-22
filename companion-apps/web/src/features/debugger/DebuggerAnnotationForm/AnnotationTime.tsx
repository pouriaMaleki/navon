import styles from "./AnnotationTime.module.css";

type Props = {
  relMs: number;
  startOffset: number;
  endOffset: number;
  formatMs: (ms: number) => string;
};

export const AnnotationTime = ({ relMs, startOffset, endOffset, formatMs }: Props) => (
  <div className={styles.time}>
    At {formatMs(relMs)} (range: {formatMs(relMs + startOffset)} – {formatMs(relMs + endOffset)}
    )
  </div>
);
