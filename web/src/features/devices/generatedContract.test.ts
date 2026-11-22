import { expectTypeOf, it } from "vitest";
import type { Device as WireDevice } from "../../generated/Device";
import type { DeviceActionInput } from "../../generated/DeviceActionInput";
import type { DeviceActionOutput } from "../../generated/DeviceActionOutput";
import type { DeviceDetail as WireDetail } from "../../generated/DeviceDetail";
import type { DeviceList as WireList } from "../../generated/DeviceList";
import type { ActionResult, ActionType, Device, DeviceAction, DeviceDetail, DeviceList } from "./contract";

/** The hand-written device contract must keep accepting everything the service sends; the web adds only `frozen` and `crashed`. */
it("accepts every shape the generated service types describe", () => {
  expectTypeOf<WireDevice>().toMatchTypeOf<Device>();
  expectTypeOf<WireDetail>().toMatchTypeOf<DeviceDetail>();
  expectTypeOf<WireList>().toMatchTypeOf<DeviceList>();
  expectTypeOf<WireDetail["actions"][number]>().toEqualTypeOf<ActionType>();
  expectTypeOf<DeviceActionInput>().toEqualTypeOf<DeviceAction>();
  expectTypeOf<Omit<DeviceActionOutput, "ok">>().toMatchTypeOf<Omit<ActionResult, "ok">>();
});
