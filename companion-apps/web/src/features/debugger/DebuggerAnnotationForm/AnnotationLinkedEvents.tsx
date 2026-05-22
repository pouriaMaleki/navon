import styles from "./AnnotationLinkedEvents.module.css";

type Props = {
  eventIds: string[];
};

export const AnnotationLinkedEvents = ({ eventIds }: Props) => {
  if (eventIds.length === 0) return null;

  return <div className={styles.linked}>Linked events: {eventIds.join(", ")}</div>;
};
