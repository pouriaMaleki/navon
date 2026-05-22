import type { HTMLAttributes, ReactNode } from "react";
import styles from "./Overlay.module.css";

type Props = {
  position: "top" | "bottom";
  children: ReactNode;
} & HTMLAttributes<HTMLDivElement>;

export const Overlay = ({ position, children, className, ...rest }: Props) => (
  <div
    className={[position === "top" ? styles.top : styles.bottom, className]
      .filter(Boolean)
      .join(" ")}
    {...rest}
  >
    {children}
  </div>
);
