import type { ButtonHTMLAttributes, ReactNode } from "react";
import styles from "./Button.module.css";

type Props = {
  variant?: "primary" | "ghost";
  children: ReactNode;
} & ButtonHTMLAttributes<HTMLButtonElement>;

export const Button = ({ variant = "ghost", children, className, ...rest }: Props) => (
  <button
    type="button"
    className={[styles.btn, variant === "primary" ? styles.primary : styles.ghost, className]
      .filter(Boolean)
      .join(" ")}
    {...rest}
  >
    {children}
  </button>
);
