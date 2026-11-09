import { SettingRow } from "../rows/SettingRow";

export const heading = (title: string) => (
  <h1 className="sr-only" tabIndex={-1} data-page-title>
    {title}
  </h1>
);

export function Toggle({ title, description, on, set }: { title: string; description?: string; on: boolean; set(on: boolean): void }) {
  return (
    <SettingRow title={title} description={description}>
      <input type="checkbox" role="switch" aria-label={title} checked={on} onChange={(event) => set(event.target.checked)} />
    </SettingRow>
  );
}
