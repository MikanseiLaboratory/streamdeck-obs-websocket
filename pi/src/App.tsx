import { useEffect, useMemo, useState } from "react";
import {
  useGlobalSettings,
  usePluginMessage,
  useSendToPlugin,
  useSettings,
  useStreamDeck
} from "@mikanseilaboratory/streamdeck-pi-client";
import type {
  ActionParams,
  ActionSettings,
  GlobalSettings,
  InstanceConfig,
  TargetGroup,
  TargetSelector
} from "./generated/contracts";

const PALETTE = ["#4c8dff", "#ef5b5b", "#3cba7a", "#e2b15a", "#b07cff", "#4ec8d4", "#f08bbd", "#9aa4b5"];

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

type Status = { kind: string; message?: string; obsVersion?: string };
type InstanceStatus = InstanceConfig & { status?: Status };
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

export function App() {
  const deck = useStreamDeck();
  const kind = deck.actionInfo?.action.split(".").at(-1) ?? "stream";
  const global = useGlobalSettings<GlobalSettings>(globalDefaults);
  const action = useSettings<ActionSettings>(actionDefaults);
  const send = useSendToPlugin();
  const [statuses, setStatuses] = useState<InstanceStatus[]>([]);
  const [catalog, setCatalog] = useState<CatalogItem[]>([]);

  usePluginMessage((payload: { type?: string; instances?: InstanceStatus[]; items?: CatalogItem[] }) => {
    if (payload.type === "status" && payload.instances) setStatuses(payload.instances);
    if (payload.type === "catalog" && payload.items) setCatalog(payload.items);
  });

  useEffect(() => {
    send({ type: "ready" });
    const resource = CATALOG[kind];
    if (resource) send({ type: "query", resource });
  }, [send, kind, action.settings.common.target, action.settings.shared.sourceName]);

  const fields = KIND_FIELDS[kind] ?? [];
  const instances = global.settings.instances ?? [];

  return (
    <div className="app">
      <Connections
        settings={global.settings}
        setSettings={global.setSettings}
        statuses={statuses}
        reconnect={(id) => send({ type: "reconnect", id })}
      />
      <section className="panel">
        <h2>Target</h2>
        <TargetPicker
          instances={instances}
          groups={global.settings.groups ?? []}
          value={action.settings.common?.target ?? { kind: "all" }}
          onChange={(target) =>
            action.setSettings((previous) => ({
              ...previous,
              common: { ...previous.common, target }
            }))
          }
        />
        {fields.length > 0 && (
          <label className="row">
            <input
              type="checkbox"
              checked={action.settings.common?.sharedParams !== false}
              onChange={(event) =>
                action.setSettings((previous) => ({
                  ...previous,
                  common: { ...previous.common, sharedParams: event.target.checked }
                }))
              }
            />
            Use the same parameters for every target
          </label>
        )}
        <label className="row">
          <input
            type="checkbox"
            checked={!!action.settings.advanced?.longPress}
            onChange={(event) =>
              action.setSettings((previous) => ({
                ...previous,
                advanced: { ...previous.advanced, longPress: event.target.checked }
              }))
            }
          />
          Long press uses the alternate action
        </label>
      </section>
      {fields.length > 0 && (
        <Params
          kind={kind}
          fields={fields}
          shared={action.settings.common?.sharedParams !== false}
          instances={instances}
          settings={action.settings}
          setSettings={action.setSettings}
          catalog={catalog}
        />
      )}
    </div>
  );
}

function Connections({
  settings,
  setSettings,
  statuses,
  reconnect
}: {
  settings: GlobalSettings;
  setSettings: (next: GlobalSettings | ((previous: GlobalSettings) => GlobalSettings)) => void;
  statuses: InstanceStatus[];
  reconnect: (id: string) => void;
}) {
  const instances = settings.instances ?? [];
  const statusOf = (id: string) => statuses.find((item) => item.id === id)?.status;

  const update = (next: InstanceConfig[]) => setSettings((previous) => ({ ...previous, instances: next }));

  return (
    <section className="panel">
      <h2>OBS instances</h2>
      {instances.map((instance, index) => {
        const status = statusOf(instance.id);
        return (
          <div
            className="instance"
            key={instance.id}
            draggable
            onDragStart={(event) => event.dataTransfer.setData("text/plain", String(index))}
            onDragOver={(event) => event.preventDefault()}
            onDrop={(event) => {
              const from = Number(event.dataTransfer.getData("text/plain"));
              if (Number.isNaN(from) || from === index) return;
              const next = instances.slice();
              const [moved] = next.splice(from, 1);
              next.splice(index, 0, moved);
              update(next);
            }}
          >
            <span className="dot" style={{ background: dotColor(status) }} title={status?.kind ?? "unknown"} />
            <div className="fields">
              <span>Name</span>
              <input value={instance.name} onChange={(event) => patchInstance(instances, index, { name: event.target.value }, update)} />
              <span>Host</span>
              <input value={instance.host} onChange={(event) => patchInstance(instances, index, { host: event.target.value }, update)} />
              <span>Port</span>
              <input type="number" value={instance.port} onChange={(event) => patchInstance(instances, index, { port: Number(event.target.value) }, update)} />
              <span>Password</span>
              <input type="password" value={instance.password} onChange={(event) => patchInstance(instances, index, { password: event.target.value }, update)} />
            </div>
            <div className="stack">
              <input className="color" type="color" value={instance.color} onChange={(event) => patchInstance(instances, index, { color: event.target.value }, update)} />
              <button onClick={() => reconnect(instance.id)}>Reconnect</button>
              <button onClick={() => update(instances.filter((_, item) => item !== index))}>Remove</button>
            </div>
          </div>
        );
      })}
      <button
        onClick={() =>
          update([
            ...instances,
            {
              id: crypto.randomUUID(),
              name: `OBS ${instances.length + 1}`,
              host: "127.0.0.1",
              port: 4455 + instances.length,
              password: "",
              color: PALETTE[instances.length % PALETTE.length],
              enabled: true
            }
          ])
        }
      >
        Add OBS
      </button>
      <Groups
        instances={instances}
        groups={settings.groups ?? []}
        setGroups={(groups) => setSettings((previous) => ({ ...previous, groups }))}
      />
      <label className="row">
        Long press
        <input
          type="number"
          value={settings.longPressMs}
          onChange={(event) => setSettings((previous) => ({ ...previous, longPressMs: Number(event.target.value) }))}
        />
        ms
      </label>
    </section>
  );
}

function Groups({
  instances,
  groups,
  setGroups
}: {
  instances: InstanceConfig[];
  groups: TargetGroup[];
  setGroups: (groups: TargetGroup[]) => void;
}) {
  return (
    <div className="stack">
      <strong>Groups</strong>
      {groups.map((group, index) => (
        <div className="stack" key={group.id}>
          <div className="row">
            <input
              className="grow"
              value={group.name}
              onChange={(event) => {
                const next = groups.slice();
                next[index] = { ...group, name: event.target.value };
                setGroups(next);
              }}
            />
            <button onClick={() => setGroups(groups.filter((_, item) => item !== index))}>Remove</button>
          </div>
          <div className="chips">
            {instances.map((instance) => {
              const on = group.members.includes(instance.id);
              return (
                <button
                  key={instance.id}
                  className={on ? "chip on" : "chip"}
                  onClick={() => {
                    const members = on ? group.members.filter((id) => id !== instance.id) : [...group.members, instance.id];
                    const next = groups.slice();
                    next[index] = { ...group, members };
                    setGroups(next);
                  }}
                >
                  {instance.name}
                </button>
              );
            })}
          </div>
        </div>
      ))}
      <button
        onClick={() => setGroups([...groups, { id: crypto.randomUUID(), name: `Group ${groups.length + 1}`, members: [] }])}
      >
        Add group
      </button>
    </div>
  );
}

function TargetPicker({
  instances,
  groups,
  value,
  onChange
}: {
  instances: InstanceConfig[];
  groups: TargetGroup[];
  value: TargetSelector;
  onChange: (value: TargetSelector) => void;
}) {
  const selected = value.kind === "instances" ? value.ids : [];
  return (
    <div className="stack">
      <div className="chips">
        <button className={value.kind === "all" ? "chip on" : "chip"} onClick={() => onChange({ kind: "all" })}>
          All enabled
        </button>
        {groups.map((group) => (
          <button
            key={group.id}
            className={value.kind === "group" && value.id === group.id ? "chip on" : "chip"}
            onClick={() => onChange({ kind: "group", id: group.id })}
          >
            {group.name}
          </button>
        ))}
      </div>
      <div className="chips">
        {instances.map((instance) => {
          const on = value.kind === "instances" && selected.includes(instance.id);
          return (
            <button
              key={instance.id}
              className={on ? "chip on" : "chip"}
              style={{ borderColor: instance.color }}
              onClick={() => {
                const ids = on ? selected.filter((id) => id !== instance.id) : [...selected, instance.id];
                onChange(ids.length === 0 ? { kind: "all" } : { kind: "instances", ids });
              }}
            >
              {instance.name}
            </button>
          );
        })}
      </div>
    </div>
  );
}

function Params({
  kind,
  fields,
  shared,
  instances,
  settings,
  setSettings,
  catalog
}: {
  kind: string;
  fields: Array<keyof ActionParams>;
  shared: boolean;
  instances: InstanceConfig[];
  settings: ActionSettings;
  setSettings: (next: ActionSettings | ((previous: ActionSettings) => ActionSettings)) => void;
  catalog: CatalogItem[];
}) {
  const editors = shared ? [{ id: "shared", name: "Shared", params: settings.shared ?? emptyParams() }] : instances.map((instance) => ({
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

  return (
    <section className="panel">
      <h2>Parameters</h2>
      {editors.map((editor) => (
        <div key={editor.id} className="stack">
          {!shared && <strong>{editor.name}</strong>}
          <ParamFields kind={kind} fields={fields} params={editor.params} suggestions={suggestions} onChange={(params) => write(editor.id, params)} />
        </div>
      ))}
      {catalog.some((item) => item.missingOn.length > 0) && (
        <p className="warn">
          Some names exist on only part of the selection:{" "}
          {catalog
            .filter((item) => item.missingOn.length > 0)
            .map((item) => item.name)
            .join(", ")}
        </p>
      )}
    </section>
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
    <div className="fields">
      <datalist id={listId}>
        {suggestions.map((name) => (
          <option key={name} value={name} />
        ))}
      </datalist>
      {fields.map((field) => (
        <Field key={field} field={field} params={params} listId={listId} onChange={onChange} />
      ))}
    </div>
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
    return (
      <>
        <span>{label(field)}</span>
        <input type="checkbox" checked={value} onChange={(event) => onChange({ ...params, [field]: event.target.checked })} />
      </>
    );
  }
  if (field === "mediaAction" || field === "stat") {
    const options = field === "stat" ? ["fps", "cpu", "memory", "dropped"] : ["toggle", "play", "pause", "stop", "restart", "next", "previous"];
    return (
      <>
        <span>{label(field)}</span>
        <select value={String(value)} onChange={(event) => onChange({ ...params, [field]: event.target.value })}>
          {options.map((option) => (
            <option key={option}>{option}</option>
          ))}
        </select>
      </>
    );
  }
  const numeric = field === "stepDb";
  const wide = field === "requestData" || field === "batchRequests";
  return (
    <>
      <span>{label(field)}</span>
      {wide ? (
        <textarea rows={4} value={String(value)} onChange={(event) => onChange({ ...params, [field]: event.target.value })} />
      ) : (
        <input
          type={numeric ? "number" : "text"}
          list={numeric ? undefined : listId}
          value={String(value)}
          step={numeric ? "0.5" : undefined}
          onChange={(event) =>
            onChange({ ...params, [field]: numeric ? Number(event.target.value) : event.target.value })
          }
        />
      )}
    </>
  );
}

function patchInstance(
  instances: InstanceConfig[],
  index: number,
  patch: Partial<InstanceConfig>,
  update: (next: InstanceConfig[]) => void
) {
  const next = instances.slice();
  next[index] = { ...instances[index], ...patch };
  update(next);
}

function dotColor(status: Status | undefined) {
  switch (status?.kind) {
    case "connected":
      return "#3cba7a";
    case "connecting":
      return "#e2b15a";
    case "authFailed":
      return "#ef5b5b";
    case "disabled":
      return "#6d7890";
    default:
      return "#8a4a3a";
  }
}

function label(field: keyof ActionParams) {
  return field.replace(/[A-Z]/g, (char) => ` ${char.toLowerCase()}`);
}
