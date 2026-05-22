import type { HTMLAttributes, ReactNode } from "react";
import styles from "./Card.module.css";

type Props = {
  children: ReactNode;
  className?: string;
} & HTMLAttributes<HTMLDivElement>;

export const Card = ({ children, className, ...rest }: Props) => (
  <div className={[styles.card, className].filter(Boolean).join(" ")} {...rest}>
    {children}
  </div>
);
