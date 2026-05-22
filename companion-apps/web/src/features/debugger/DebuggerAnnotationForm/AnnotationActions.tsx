import styles from "./AnnotationActions.module.css";

type Props = {
  onSave: () => void;
  onCancel: () => void;
};

export const AnnotationActions = ({ onSave, onCancel }: Props) => (
  <div className={styles.actions}>
    <button type="button" className={[styles.btn, styles.btnSave].join(" ")} onClick={onSave}>
      Save
    </button>
    <button type="button" className={styles.btn} onClick={onCancel}>
      Cancel
    </button>
  </div>
);
