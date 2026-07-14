import type { ReactNode } from "react";
import "./storyworld.css";

export default function StoryWorld({ children }: { children?: ReactNode }) {
  return (
    <div className="mstory">
      <div className="mstory__world" aria-hidden="true" />
      {children}
    </div>
  );
}
