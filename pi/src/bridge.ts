import type { GlobalSettings } from "./generated/contracts";

export type Status = { kind: string; message?: string; obsVersion?: string };

export type LiveInstance = {
  id: string;
  name?: string;
  status?: Status;
};

export type ObsConfigBridge = {
  getSettings: () => GlobalSettings;
  setSettings: (settings: GlobalSettings) => void;
  reconnect: (id?: string) => void;
  subscribe: (listener: (instances: LiveInstance[]) => void) => () => void;
};

declare global {
  interface Window {
    obsConfig?: ObsConfigBridge;
    refreshObsConfig?: (settings: GlobalSettings, instances: LiveInstance[]) => void;
  }
}
