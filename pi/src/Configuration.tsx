import { useEffect, useState } from "react";
import type { LiveInstance } from "./bridge";
import type { GlobalSettings, InstanceConfig, TargetGroup } from "./generated/contracts";

const PALETTE = ["#4c8dff", "#ef5b5b", "#3cba7a", "#e2b15a", "#b07cff", "#4ec8d4", "#f08bbd", "#9aa4b5"];

const emptySettings = (): GlobalSettings => ({
  instances: [],
  groups: [],
  longPressMs: 500,
  fgColor: "#f4f7fb"
});

export function Configuration() {
  const [settings, setSettings] = useState<GlobalSettings>(emptySettings);
  const [statuses, setStatuses] = useState<LiveInstance[]>([]);
  const [ready, setReady] = useState(false);
  const [missing, setMissing] = useState(false);

  useEffect(() => {
    window.refreshObsConfig = (next, instances) => {
      setSettings(next);
      setStatuses(instances);
      setReady(true);
    };
    if (!window.opener) {
      setMissing(true);
      return () => {
        delete window.refreshObsConfig;
      };
    }
    let unsubscribe = () => {};
    const attach = () => {
      const bridge = window.opener?.obsConfig;
      if (!bridge) return false;
      setSettings(bridge.getSettings());
      unsubscribe = bridge.subscribe(setStatuses);
      setReady(true);
      return true;
    };
    if (attach()) {
      return () => {
        unsubscribe();
        delete window.refreshObsConfig;
      };
    }
    const timer = window.setInterval(() => {
      if (attach()) window.clearInterval(timer);
    }, 200);
    return () => {
      window.clearInterval(timer);
      unsubscribe();
      delete window.refreshObsConfig;
    };
  }, []);

  const commit = (next: GlobalSettings) => {
    setSettings(next);
    window.opener?.obsConfig?.setSettings(next);
  };

  const instances = settings.instances ?? [];
  const updateInstances = (next: InstanceConfig[]) => commit({ ...settings, instances: next });

  if (missing) {
    return (
      <main>
        <header className="config-header">
          <h1>OBS Websocket [MikanseiLaboratory]</h1>
        </header>
        <p className="empty-note">Open this window from a key's property inspector.</p>
      </main>
    );
  }

  return (
    <main>
      <header className="config-header">
        <h1>OBS Websocket [MikanseiLaboratory]</h1>
      </header>
      <div className="config-body">
        <div className="sdpi-heading">OBS instances</div>
        {!ready && <p className="empty-note">Waiting for the property inspector…</p>}
        {ready && instances.length === 0 && <p className="empty-note">No OBS instances yet. Add one to start controlling it from a key.</p>}
        <div className="obs-grid">
          {instances.map((instance, index) => (
            <InstanceCard
              key={instance.id}
              instance={instance}
              index={index}
              status={statuses.find((item) => item.id === instance.id)}
              onChange={(next) => {
                const copy = instances.slice();
                copy[index] = next;
                updateInstances(copy);
              }}
              onRemove={() => updateInstances(instances.filter((_, item) => item !== index))}
              onReconnect={() => window.opener?.obsConfig?.reconnect(instance.id)}
              onDrop={(from) => {
                if (from === index) return;
                const copy = instances.slice();
                const [moved] = copy.splice(from, 1);
                copy.splice(index, 0, moved);
                updateInstances(copy);
              }}
            />
          ))}
        </div>
        <div className="card-actions">
          <button
            type="button"
            onClick={() =>
              updateInstances([
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
          <button type="button" onClick={() => window.opener?.obsConfig?.reconnect()}>
            Reconnect all
          </button>
        </div>
        <Groups
          instances={instances}
          groups={settings.groups ?? []}
          setGroups={(groups) => commit({ ...settings, groups })}
        />
        <div className="sdpi-heading">Plugin</div>
        <div className="sdpi-item">
          <div className="sdpi-item-label">Long press</div>
          <input
            className="sdpi-item-value"
            type="number"
            min={100}
            value={settings.longPressMs}
            onChange={(event) => commit({ ...settings, longPressMs: Number(event.target.value) })}
          />
        </div>
      </div>
    </main>
  );
}

function InstanceCard({
  instance,
  index,
  status,
  onChange,
  onRemove,
  onReconnect,
  onDrop
}: {
  instance: InstanceConfig;
  index: number;
  status?: LiveInstance;
  onChange: (instance: InstanceConfig) => void;
  onRemove: () => void;
  onReconnect: () => void;
  onDrop: (from: number) => void;
}) {
  const kind = instance.enabled ? status?.status?.kind : "disabled";
  return (
    <section
      className="obs-card"
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        const from = Number(event.dataTransfer.getData("text/plain"));
        if (!Number.isNaN(from)) onDrop(from);
      }}
    >
      <div className="obs-card-head">
        <span
          className="drag-handle"
          draggable
          title="Drag to reorder"
          onDragStart={(event) => event.dataTransfer.setData("text/plain", String(index))}
        >
          :::
        </span>
        <span className={`status-dot ${kind ?? ""}`} title={kind ?? "unknown"} />
        <strong>{instance.name || "OBS"}</strong>
        <span className="status-text">{statusLabel(kind, status?.status)}</span>
        <input type="color" value={instance.color} aria-label="Color" onChange={(event) => onChange({ ...instance, color: event.target.value })} />
      </div>
      <TextRow label="Name" value={instance.name} onChange={(name) => onChange({ ...instance, name })} />
      <TextRow label="Host" value={instance.host} onChange={(host) => onChange({ ...instance, host })} />
      <div className="sdpi-item">
        <div className="sdpi-item-label">Port</div>
        <input
          className="sdpi-item-value"
          type="number"
          value={instance.port}
          onChange={(event) => onChange({ ...instance, port: Number(event.target.value) })}
        />
      </div>
      <div className="sdpi-item">
        <div className="sdpi-item-label">Password</div>
        <input
          className="sdpi-item-value"
          type="password"
          value={instance.password}
          onChange={(event) => onChange({ ...instance, password: event.target.value })}
        />
      </div>
      <div type="checkbox" className="sdpi-item">
        <div className="sdpi-item-label">Enabled</div>
        <input
          id={`enabled-${instance.id}`}
          className="sdpi-item-value"
          type="checkbox"
          checked={instance.enabled}
          onChange={(event) => onChange({ ...instance, enabled: event.target.checked })}
        />
        <label htmlFor={`enabled-${instance.id}`}>
          <span></span>
          Connect to this OBS
        </label>
      </div>
      <div className="card-actions">
        <button type="button" onClick={onReconnect}>
          Reconnect
        </button>
        <button type="button" onClick={onRemove}>
          Remove
        </button>
      </div>
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
    <>
      <div className="sdpi-heading">Groups</div>
      {groups.map((group, index) => (
        <section className="group-card" key={group.id}>
          <TextRow
            label="Name"
            value={group.name}
            onChange={(name) => {
              const next = groups.slice();
              next[index] = { ...group, name };
              setGroups(next);
            }}
          />
          <div type="checkbox" className="sdpi-item targets">
            <div className="sdpi-item-label">Members</div>
            <div className="sdpi-item-value">
              {instances.map((instance) => {
                const id = `group-${group.id}-${instance.id}`;
                const on = group.members.includes(instance.id);
                return (
                  <div className="sdpi-item-child" key={instance.id}>
                    <input
                      id={id}
                      type="checkbox"
                      checked={on}
                      onChange={() => {
                        const members = on ? group.members.filter((item) => item !== instance.id) : [...group.members, instance.id];
                        const next = groups.slice();
                        next[index] = { ...group, members };
                        setGroups(next);
                      }}
                    />
                    <label htmlFor={id}>
                      <span></span>
                      <i className="swatch" style={{ background: instance.color }} />
                      {instance.name || "OBS"}
                    </label>
                  </div>
                );
              })}
            </div>
          </div>
          <div className="card-actions">
            <button type="button" onClick={() => setGroups(groups.filter((_, item) => item !== index))}>
              Remove group
            </button>
          </div>
        </section>
      ))}
      <div className="card-actions">
        <button
          type="button"
          onClick={() => setGroups([...groups, { id: crypto.randomUUID(), name: `Group ${groups.length + 1}`, members: [] }])}
        >
          Add group
        </button>
      </div>
    </>
  );
}

function TextRow({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  return (
    <div className="sdpi-item">
      <div className="sdpi-item-label">{label}</div>
      <input className="sdpi-item-value" type="text" value={value} onChange={(event) => onChange(event.target.value)} />
    </div>
  );
}

function statusLabel(kind: string | undefined, status: LiveInstance["status"]) {
  switch (kind) {
    case "connected":
      return status?.obsVersion ? `Connected · ${status.obsVersion}` : "Connected";
    case "connecting":
      return "Connecting";
    case "authFailed":
      return "Authentication failed";
    case "unreachable":
      return "Unreachable";
    case "disabled":
      return "Disabled";
    default:
      return "Not connected";
  }
}
