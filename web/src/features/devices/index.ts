import { lazy } from "react";

// Front door: the only module other features import from devices.
export { DevicesPanel } from "./DevicesPanel";
export { DevicesFloat } from "./FloatingDevice";
export { devicesApi, setDevicesApi, useDevices, useDevicesEntry, useDevicesOffered, useApiVersion, type DevicesEntry } from "./api";
export { devicesPlannedApi, setDevicesPlannedApi, usePlannedApiVersion, usePlannedSupport } from "./planned";
export type { Device, DeviceAction, DeviceDetail, DeviceList, DevicesApi, PlatformStatus } from "./contract";
export type { DeviceTool, DevicesPlannedApi, InstallProgress, ToolSnapshot, ToolVersions } from "./planned";

/** Dev-only: `#/preview/devices?frame=<id>`. The production flag is false, so the lazy chunk is never emitted. */
export const DevicesPreview = __ORBYN_PREVIEWS__ ? lazy(() => import("./DevicesPreview")) : null;
export const devicesPreviewRoute = (hash: string): boolean => __ORBYN_PREVIEWS__ && hash.startsWith("#/preview/devices");
