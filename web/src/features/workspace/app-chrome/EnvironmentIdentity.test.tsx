import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { installMockFetch } from "../../../test-support";
import type { ReleaseChannel } from "../../../generated/ReleaseChannel";
import { useUi } from "../../../product/state/ui";
import { EnvironmentIdentity } from "./EnvironmentIdentity";

let mock: MockApi;
const serve = (channel: ReleaseChannel) => mock.patchInfo({ version: "0.4.1", channel });

beforeEach(() => {
  mock = installMockFetch();
  useUi.getState().setPrefs({ environmentIdentification: "artwork" });
});
afterEach(() => vi.unstubAllGlobals());

it("shows Dev and Nightly artwork but nothing for a stable build", async () => {
  serve("dev");
  const { unmount } = render(<EnvironmentIdentity />);
  expect(await screen.findByText("Dev")).toHaveAttribute("data-channel", "dev");
  unmount();
  serve("stable");
  const { container } = render(<EnvironmentIdentity />);
  await act(async () => undefined);
  expect(container).toBeEmptyDOMElement();
});

it("shows a version pill or nothing, as chosen", async () => {
  serve("nightly");
  useUi.getState().setPrefs({ environmentIdentification: "pill" });
  const { container } = render(<EnvironmentIdentity />);
  expect(await screen.findByText("v0.4.1 Nightly")).toBeInTheDocument();
  act(() => useUi.getState().setPrefs({ environmentIdentification: "none" }));
  expect(container).toBeEmptyDOMElement();
});
