// Light / dark theme. "system" follows Windows; the others force a theme.
// The choice is not saved yet: it will be stored by the app settings (T-37).

export type ThemeChoice = "system" | "light" | "dark";

export const THEME_OPTIONS: ReadonlyArray<{ value: ThemeChoice; label: string }> = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

/** Applies the choice to the page. tokens.css reacts to the data-theme attribute. */
export function applyTheme(
  choice: ThemeChoice,
  root: Pick<Element, "setAttribute" | "removeAttribute"> = document.documentElement,
): void {
  if (choice === "system") {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", choice);
  }
}
