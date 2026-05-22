import styles from "./AnnotationSelect.module.css";

type Props = {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: readonly { value: string; label: string }[];
};

export const AnnotationSelect = ({ label, value, onChange, options }: Props) => (
  <label className={styles.label}>
    {label}
    <select value={value} onChange={(e) => onChange(e.target.value)}>
      {options.map((opt) => (
        <option key={opt.value} value={opt.value}>
          {opt.label}
        </option>
      ))}
    </select>
  </label>
);
