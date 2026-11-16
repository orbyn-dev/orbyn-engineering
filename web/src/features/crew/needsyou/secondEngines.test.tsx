import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { installMockFetch } from "../../../test-support";
import { useConnection } from "../../../product/state/connection";
import { crewApi } from "../autopilot/approvals/api";
import { NeedsYouInbox } from "./NeedsYouInbox";
import { readSecondEngineNotices } from "./secondEngines";

let mock: MockApi;
beforeEach(() => {
  mock=installMockFetch();
  mock.reset("crew-one-vendor");
  useConnection.setState({remote:false});
  window.location.hash="";
});

it("keeps the server notice across remounts, offers only sign-in/dismiss and persists dismissal",async()=>{
  const user=userEvent.setup();
  let view=render(<NeedsYouInbox/>);
  const title=/Sign in to a second engine to allow:/;
  let row=(await screen.findAllByText(title))[0]!.closest("li")!;
  expect(within(row).getAllByRole("button").map((button)=>button.textContent)).toEqual(["Sign in to a second engine","Dismiss"]);
  view.unmount();
  view=render(<NeedsYouInbox/>);
  row=(await screen.findAllByText(title))[0]!.closest("li")!;
  const target=within(row).getByText(title).textContent!;
  await user.click(within(row).getByRole("button",{name:/^Sign in/}));
  expect(window.location.hash).toBe("#/settings/providers");
  expect(screen.getByText(target)).toBeVisible();
  await user.click(within(row).getByRole("button",{name:/^Dismiss/}));
  await waitFor(()=>expect(screen.queryByText(target)).toBeNull());
  view.unmount();
  render(<NeedsYouInbox/>);
  await screen.findByRole("list",{name:"Needs you"});
  expect(screen.queryByText(target)).toBeNull();
  const writes=vi.mocked(fetch).mock.calls.filter(([,init])=>init?.method && init.method!=="GET");
  expect(writes).toHaveLength(1);
  expect(String(writes[0]![0])).toMatch(/second-engine\/\d+\/dismiss/);
});

it("follows the immutable cursor even when a page only resolved notices",async()=>{
  const read=vi.spyOn(crewApi,"secondEngineNotices")
    .mockResolvedValueOnce({items:[],next_before:80})
    .mockResolvedValueOnce({items:[{id:79,task_id:"task",at:"2026-11-16T00:00:00Z",target:"install zod",lead_engine:"alpha",lead_family:"alpha"}],next_before:null});
  const items=await readSecondEngineNotices();
  expect(items.map((item)=>item.id)).toEqual([79]);
  expect(read.mock.calls.map(([before])=>before)).toEqual([null,80]);
  read.mockRestore();
});
