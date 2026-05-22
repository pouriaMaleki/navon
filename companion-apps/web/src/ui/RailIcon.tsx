import type { ButtonHTMLAttributes, ReactNode } from "react";
import styles from "./RailIcon.module.css";

type Props = {
  label: string;
  children: ReactNode;
} & ButtonHTMLAttributes<HTMLButtonElement>;

export const RailIcon = ({ label, children, className, ...rest }: Props) => (
  <button
    type="button"
    className={[styles.icon, className].filter(Boolean).join(" ")}
    aria-label={label}
    {...rest}
  >
    {children}
  </button>
);
