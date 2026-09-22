import { useEffect, useMemo, useRef, useState } from "react";
import {
  useGlobalSettings,
  usePluginMessage,
  useSendToPlugin,
  useSettings,
  useStreamDeck
} from "@mikanseilaboratory/streamdeck-pi-client";
import type { LiveInstance, ObsConfigBridge } from "./bridge";
import type {
  ActionParams,
  ActionSettings,
  GlobalSettings,
  InstanceConfig,
  TargetGroup,
  TargetSelector
} from "./generated/contracts";

const emptyParams = (): ActionParams => ({
  sceneName: "",
  sourceName: "",
  inputName: "",
  filterName: "",
  collectionName: "",
  profileName: "",
  format: "png",
  filePath: "",
  hotkeyName: "",
  keyId: "",
  shift: false,
  control: false,
  alt: false,
  command: false,
  chapterName: "",
  mediaAction: "toggle",
  stat: "fps",
  stepDb: 1,
  requestType: "",
  requestData: "{}",
  batchRequests: "[]",
  haltOnFailure: false
});

const actionDefaults: ActionSettings = {
  common: { target: { kind: "all" }, sharedParams: true },
  advanced: { longPress: false, longPressMs: 0 },
  shared: emptyParams(),
  params: {}
};

const globalDefaults: GlobalSettings = {
  instances: [],
  groups: [],
  longPressMs: 500,
  fgColor: "#f4f7fb"
};

type CatalogItem = { name: string; presentOn: string[]; missingOn: string[] };

const KIND_FIELDS: Record<string, Array<keyof ActionParams>> = {
  scene: ["sceneName"],
  source: ["sceneName", "sourceName"],
  mute: ["inputName"],
  filter: ["sourceName", "filterName"],
  collection: ["collectionName"],
  profile: ["profileName"],
  screenshot: ["sourceName", "format", "filePath"],
  hotkey: ["hotkeyName", "keyId", "shift", "control", "alt", "command"],
  refreshbrowser: ["inputName"],
  refreshcapture: ["inputName"],
  chapter: ["chapterName"],
  media: ["inputName", "mediaAction"],
  stats: ["stat"],
  volume: ["inputName", "stepDb"],
  raw: ["requestType", "requestData"],
  rawbatch: ["batchRequests", "haltOnFailure"]
};

const CATALOG: Record<string, string> = {
  scene: "scenes",
  source: "scenes",
  mute: "inputs",
  filter: "filters",
  collection: "collections",
  profile: "profiles",
  screenshot: "scenes",
  hotkey: "hotkeys",
  refreshbrowser: "inputs",
  refreshcapture: "inputs",
  media: "inputs",
  volume: "inputs"
};

const CONFIG_WINDOW = "obs-websocket-config";

export function App() {
  const deck = useStreamDeck();
  const kind = deck.actionInfo?.action.split(".").at(-1) ?? "stream";
  const global = useGlobalSettings<GlobalSettings>(globalDefaults);
  const action = useSettings<ActionSettings>(actionDefaults);
  const send = useSendToPlugin();
  const [statuses, setStatuses] = useState<LiveInstance[]>([]);
  const [catalog, setCatalog] = useState<CatalogItem[]>([]);
  const settingsRef = useRef(global.settings);
  const statusRef = useRef(statuses);
  const listenersRef = useRef(new Set<(instances: LiveInstance[]) => void>());
  const configWindowRef = useRef<Window | null>(null);
  const sendRef = useRef(send);
  settingsRef.current = global.settings;
  statusRef.current = statuses;
  sendRef.current = send;

  usePluginMessage((payload: { type?: string; instances?: LiveInstance[]; items?: CatalogItem[] }) => {
    if (payload.type === "status" && payload.instances) setStatuses(payload.instances);
    if (payload.type === "catalog" && payload.items) setCatalog(payload.items);
  });

  useEffect(() => {
    const bridge: ObsConfigBridge = {
      getSettings: () => settingsRef.current,
      setSettings: (next) => global.setSettings(next),
      reconnect: (id) => {
        const ids = id ? [id] : settingsRef.current.instances.map((instance) => instance.id);
        for (const item of ids) sendRef.current({ type: "reconnect", id: item });
      },
      subscribe: (listener) => {
        listenersRef.current.add(listener);
        listener(statusRef.current);
        return () => listenersRef.current.delete(listener);
      }
    };
    window.obsConfig = bridge;
    return () => {
      if (window.obsConfig === bridge) delete window.obsConfig;
    };
  }, [global.setSettings]);

  useEffect(() => {
    for (const listener of listenersRef.current) listener(statuses);
    const child = configWindowRef.current;
    if (child && !child.closed && child.opener === window) {
      child.refreshObsConfig?.(settingsRef.current, statuses);
    }
  }, [statuses, global.settings]);

  useEffect(() => {
    send({ type: "ready" });
    const resource = CATALOG[kind];
    if (resource) send({ type: "query", resource });
  }, [send, kind, action.settings.common.target, action.settings.shared.sourceName]);

  const fields = KIND_FIELDS[kind] ?? [];
  const instances = global.settings.instances ?? [];
  const groups = global.settings.groups ?? [];
  const target = action.settings.common?.target ?? { kind: "all" };

  const openConfig = () => {
    const features = "width=760,height=840";
    const existing = window.open("", CONFIG_WINDOW);
    if (!existing) return;
    let owned = false;
    try {
      owned = existing.opener === window;
    } catch {
      owned = false;
    }
    if (!owned) {
      existing.close();
      configWindowRef.current = window.open("./configuration.html", CONFIG_WINDOW, features);
      return;
    }
    const href = existing.location.href;
    if (href.includes("configuration.html")) {
      existing.focus();
      existing.refreshObsConfig?.(settingsRef.current, statusRef.current);
      configWindowRef.current = existing;
      return;
    }
    existing.location.href = new URL("./configuration.html", window.location.href).href;
    configWindowRef.current = existing;
  };

  return (
    <div className="sdpi-wrapper">
      <div className="sdpi-heading">Target</div>
      <TargetPicker
        instances={instances}
        groups={groups}
        statuses={statuses}
        value={target}
        onChange={(next) =>
          action.setSettings((previous) => ({
            ...previous,
            common: { ...previous.common, target: next }
          }))
        }
      />
      {fields.length > 0 && (
        <CheckRow
          label="Parameters"
          checked={action.settings.common?.sharedParams !== false}
          text="Same values for every target"
          onChange={(checked) =>
            action.setSettings((previous) => ({
              ...previous,
              common: { ...previous.common, sharedParams: checked }
            }))
          }
        />
      )}
      <CheckRow
        label="Long press"
        checked={!!action.settings.advanced?.longPress}
        text="Use the alternate action"
        onChange={(checked) =>
          action.setSettings((previous) => ({
            ...previous,
            advanced: { ...previous.advanced, longPress: checked }
          }))
        }
      />
      {fields.length > 0 && (
        <Params
          kind={kind}
          fields={fields}
          shared={action.settings.common?.sharedParams !== false}
          instances={instances}
          target={target}
          groups={groups}
          settings={action.settings}
          setSettings={action.setSettings}
          catalog={catalog}
        />
      )}
      <div className="sdpi-heading">Connections</div>
      <div className="sdpi-item">
        <div className="sdpi-item-label">OBS</div>
        <button className="sdpi-item-value" type="button" onClick={openConfig}>
          Manage instances
        </button>
      </div>
    </div>
  );
}

function TargetPicker({
  instances,
  groups,
  statuses,
  value,
  onChange
}: {
  instances: InstanceConfig[];
  groups: TargetGroup[];
  statuses: LiveInstance[];
  value: TargetSelector;
  onChange: (value: TargetSelector) => void;
}) {
  const [preferMultiple, setPreferMultiple] = useState(value.kind === "instances" && value.ids.length !== 1);
  useEffect(() => {
    if (value.kind !== "instances") setPreferMultiple(false);
  }, [value]);
  const mode = preferMultiple ? "multiple" : targetMode(value);
  const selected = value.kind === "instances" ? value.ids : [];
  return (
    <>
      <div type="select" className="sdpi-item">
        <div className="sdpi-item-label">Send to</div>
        <select
          className="sdpi-item-value select"
          value={mode}
          onChange={(event) => {
            const next = event.target.value;
            setPreferMultiple(next === "multiple");
            onChange(selectorFromMode(next, selected));
          }}
        >
          <option value="all">All enabled</option>
          {groups.map((group) => (
            <option key={group.id} value={`group:${group.id}`}>
              Group: {group.name || "Untitled"}
            </option>
          ))}
          {instances.map((instance) => (
            <option key={instance.id} value={`instance:${instance.id}`}>
              {instance.name || "OBS"}
              {instance.enabled ? "" : " (disabled)"}
            </option>
          ))}
          <option value="multiple">Multiple instances</option>
        </select>
      </div>
      {mode === "multiple" && (
        <div type="checkbox" className="sdpi-item targets">
          <div className="sdpi-item-label">Instances</div>
          <div className="sdpi-item-value">
            {instances.length === 0 && <span>No instances yet</span>}
            {instances.map((instance) => {
              const id = `target-${instance.id}`;
              const on = selected.includes(instance.id);
              const status = statuses.find((item) => item.id === instance.id)?.status?.kind;
              return (
                <div className="sdpi-item-child" key={instance.id}>
                  <input
                    id={id}
                    type="checkbox"
                    checked={on}
                    onChange={() => {
                      const ids = on ? selected.filter((item) => item !== instance.id) : [...selected, instance.id];
                      onChange({ kind: "instances", ids });
                    }}
                  />
                  <label htmlFor={id}>
                    <span></span>
                    <i className={`swatch status-dot ${status ?? ""}`} style={{ background: statusColor(status, instance.color) }} />
                    {instance.name || "OBS"}
                  </label>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </>
  );
}

function CheckRow({
  label,
  checked,
  text,
  onChange
}: {
  label: string;
  checked: boolean;
  text: string;
  onChange: (checked: boolean) => void;
}) {
  const id = `check-${label.replace(/\s+/g, "-").toLowerCase()}`;
  return (
    <div type="checkbox" className="sdpi-item">
      <div className="sdpi-item-label">{label}</div>
      <input id={id} className="sdpi-item-value" type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} />
      <label htmlFor={id}>
        <span></span>
        {text}
      </label>
    </div>
  );
}

function Params({
  kind,
  fields,
  shared,
  instances,
  target,
  groups,
  settings,
  setSettings,
  catalog
}: {
  kind: string;
  fields: Array<keyof ActionParams>;
  shared: boolean;
  instances: InstanceConfig[];
  target: TargetSelector;
  groups: TargetGroup[];
  settings: ActionSettings;
  setSettings: (next: ActionSettings | ((previous: ActionSettings) => ActionSettings)) => void;
  catalog: CatalogItem[];
}) {
  const targeted = useMemo(() => instancesForTarget(instances, groups, target), [instances, groups, target]);
  const [selectedId, setSelectedId] = useState(targeted[0]?.id ?? "");
  const activeId = targeted.some((instance) => instance.id === selectedId) ? selectedId : targeted[0]?.id ?? "";
  const editors = shared
    ? [{ id: "shared", name: "Shared", params: settings.shared ?? emptyParams() }]
    : targeted
        .filter((instance) => instance.id === activeId)
        .map((instance) => ({
          id: instance.id,
          name: instance.name,
          params: settings.params?.[instance.id] ?? settings.shared ?? emptyParams()
        }));

  const write = (id: string, params: ActionParams) => {
    setSettings((previous) => {
      if (id === "shared") return { ...previous, shared: params };
      return { ...previous, params: { ...previous.params, [id]: params } };
    });
  };

  const suggestions = useMemo(() => catalog.map((item) => item.name), [catalog]);
  const partial = catalog.filter((item) => item.missingOn.length > 0);

  return (
    <>
      <div className="sdpi-heading">Action</div>
      {!shared && (
        <div type="select" className="sdpi-item">
          <div className="sdpi-item-label">Instance</div>
          <select className="sdpi-item-value select" value={activeId} onChange={(event) => setSelectedId(event.target.value)}>
            {targeted.map((instance) => (
              <option key={instance.id} value={instance.id}>
                {instance.name || "OBS"}
              </option>
            ))}
          </select>
        </div>
      )}
      {editors.map((editor) => (
        <ParamFields
          key={editor.id}
          kind={kind}
          fields={fields}
          params={editor.params}
          suggestions={suggestions}
          onChange={(params) => write(editor.id, params)}
        />
      ))}
      {partial.length > 0 && (
        <p className="caution">Some names exist on only part of the selection: {partial.map((item) => item.name).join(", ")}</p>
      )}
    </>
  );
}

function ParamFields({
  kind,
  fields,
  params,
  suggestions,
  onChange
}: {
  kind: string;
  fields: Array<keyof ActionParams>;
  params: ActionParams;
  suggestions: string[];
  onChange: (params: ActionParams) => void;
}) {
  const listId = `suggestions-${kind}`;
  return (
    <>
      <datalist id={listId}>
        {suggestions.map((name) => (
          <option key={name} value={name} />
        ))}
      </datalist>
      {fields.map((field) => (
        <Field key={field} field={field} params={params} listId={listId} onChange={onChange} />
      ))}
    </>
  );
}

function Field({
  field,
  params,
  listId,
  onChange
}: {
  field: keyof ActionParams;
  params: ActionParams;
  listId: string;
  onChange: (params: ActionParams) => void;
}) {
  const value = params[field];
  if (typeof value === "boolean") {
    const id = `field-${field}`;
    return (
      <div type="checkbox" className="sdpi-item">
        <div className="sdpi-item-label">{label(field)}</div>
        <input
          id={id}
          className="sdpi-item-value"
          type="checkbox"
          checked={value}
          onChange={(event) => onChange({ ...params, [field]: event.target.checked })}
        />
        <label htmlFor={id}>
          <span></span>
          {label(field)}
        </label>
      </div>
    );
  }
  if (field === "mediaAction" || field === "stat" || field === "format") {
    const options =
      field === "stat"
        ? ["fps", "cpu", "memory", "dropped"]
        : field === "format"
          ? ["png", "jpg", "webp"]
          : ["toggle", "play", "pause", "stop", "restart", "next", "previous"];
    return (
      <div type="select" className="sdpi-item">
        <div className="sdpi-item-label">{label(field)}</div>
        <select className="sdpi-item-value select" value={String(value)} onChange={(event) => onChange({ ...params, [field]: event.target.value })}>
          {options.map((option) => (
            <option key={option} value={option}>
              {option}
            </option>
          ))}
        </select>
      </div>
    );
  }
  const numeric = field === "stepDb";
  const wide = field === "requestData" || field === "batchRequests";
  return (
    <div className="sdpi-item">
      <div className="sdpi-item-label">{label(field)}</div>
      {wide ? (
        <textarea
          className="sdpi-item-value"
          rows={4}
          value={String(value)}
          onChange={(event) => onChange({ ...params, [field]: event.target.value })}
        />
      ) : (
        <input
          className="sdpi-item-value"
          type={numeric ? "number" : "text"}
          list={numeric ? undefined : listId}
          value={String(value)}
          step={numeric ? "0.5" : undefined}
          onChange={(event) => onChange({ ...params, [field]: numeric ? Number(event.target.value) : event.target.value })}
        />
      )}
    </div>
  );
}

function targetMode(value: TargetSelector) {
  if (value.kind === "all") return "all";
  if (value.kind === "group") return `group:${value.id}`;
  if (value.ids.length === 1) return `instance:${value.ids[0]}`;
  return "multiple";
}

function selectorFromMode(mode: string, selected: string[]): TargetSelector {
  if (mode === "all") return { kind: "all" };
  if (mode === "multiple") return { kind: "instances", ids: selected };
  if (mode.startsWith("group:")) return { kind: "group", id: mode.slice("group:".length) };
  if (mode.startsWith("instance:")) return { kind: "instances", ids: [mode.slice("instance:".length)] };
  return { kind: "all" };
}

function instancesForTarget(instances: InstanceConfig[], groups: TargetGroup[], target: TargetSelector) {
  if (target.kind === "group") {
    const group = groups.find((item) => item.id === target.id);
    return instances.filter((instance) => group?.members.includes(instance.id));
  }
  if (target.kind === "instances") return instances.filter((instance) => target.ids.includes(instance.id));
  return instances.filter((instance) => instance.enabled);
}

function statusColor(kind: string | undefined, fallback: string) {
  switch (kind) {
    case "connected":
      return "#3cba7a";
    case "connecting":
      return "#e2b15a";
    case "authFailed":
      return "#ef5b5b";
    case "disabled":
      return "#6d7890";
    case "unreachable":
      return "#8a4a3a";
    default:
      return fallback;
  }
}

function label(field: keyof ActionParams) {
  const names: Partial<Record<keyof ActionParams, string>> = {
    sceneName: "Scene",
    sourceName: "Source",
    inputName: "Input",
    filterName: "Filter",
    collectionName: "Collection",
    profileName: "Profile",
    filePath: "File",
    hotkeyName: "Hotkey",
    keyId: "Key",
    chapterName: "Chapter",
    mediaAction: "Action",
    stepDb: "Step (dB)",
    requestType: "Request",
    requestData: "Data",
    batchRequests: "Batch",
    haltOnFailure: "Halt"
  };
  return names[field] ?? field;
}
