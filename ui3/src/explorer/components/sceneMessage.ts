export const FEEDBACK_MAX = 1000;
export function feedbackHeader(sceneTitle: string, coords: string): string {
  return `Feedback on ${sceneTitle.trim() || "your scene"} (${coords}): `;
}
export function composeFeedback(sceneTitle: string, coords: string, text: string): string {
  return (feedbackHeader(sceneTitle, coords) + text.trim()).slice(0, FEEDBACK_MAX);
}
