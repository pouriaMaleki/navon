import styles from "./AnnotationNote.module.css";

type Props = {
  value: string;
  onChange: (value: string) => void;
};

export const AnnotationNote = ({ value, onChange }: Props) => (
  <label className={styles.label}>
    Note
    <textarea
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder="Describe the issue..."
      rows={3}
    />
  </label>
);
