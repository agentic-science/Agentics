"use client";

import { Download, Play, Square, Trash2, Upload, X } from "lucide-react";
import { type ChangeEvent, useEffect, useRef, useState } from "react";
import { CommunicationTimelineGraph } from "./CommunicationTimelineGraph";
import {
  type CommunicationGraph,
  cloneCommunicationGraph,
  defaultCommunicationGraph,
  deriveCommunicationTimeline,
  formatCommunicationGraphJson,
  type IndexedPoint,
  validateCommunicationGraph,
} from "./communicationGraph";
import styles from "./EasterEditor.module.css";

type EditorMode = "edit" | "presentation";

const initialGraph = cloneCommunicationGraph(defaultCommunicationGraph);

/** Renders the internal communication graph animation editor. */
export function EasterEditor() {
  const fileInputRef = useRef<HTMLInputElement>(null);
  const visualClickTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const [graph, setGraph] = useState<CommunicationGraph>(initialGraph);
  const [importErrors, setImportErrors] = useState<string[]>([]);
  const [editorMode, setEditorMode] = useState<EditorMode>("edit");
  const [selectedLinkStart, setSelectedLinkStart] =
    useState<IndexedPoint | null>(null);

  useEffect(
    () => () => {
      if (visualClickTimerRef.current !== null) {
        clearTimeout(visualClickTimerRef.current);
      }
    },
    [],
  );

  const clearVisualClickTimer = () => {
    if (visualClickTimerRef.current !== null) {
      clearTimeout(visualClickTimerRef.current);
      visualClickTimerRef.current = null;
    }
  };

  /** Applies a valid graph update. */
  const applyGraph = (nextGraph: CommunicationGraph) => {
    const result = validateCommunicationGraph(nextGraph);
    if (!result.success) {
      setImportErrors(result.errors);
      return false;
    }

    setGraph(result.data);
    setImportErrors([]);
    setSelectedLinkStart(null);
    return true;
  };

  /** Handles importing a graph JSON file. */
  const handleImport = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) {
      return;
    }

    let contents: string;
    try {
      contents = await file.text();
    } catch {
      setImportErrors([`Could not read ${file.name}.`]);
      return;
    }

    let parsed: unknown;
    try {
      parsed = JSON.parse(contents);
    } catch (error) {
      setImportErrors([
        error instanceof Error ? error.message : "Invalid JSON.",
      ]);
      return;
    }

    const result = validateCommunicationGraph(parsed);
    if (!result.success) {
      setImportErrors(result.errors);
      return;
    }

    setGraph(result.data);
    setImportErrors([]);
    setSelectedLinkStart(null);
  };

  /** Exports the current valid graph JSON. */
  const handleExport = () => {
    const blob = new Blob([formatCommunicationGraphJson(graph)], {
      type: "application/json",
    });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = "agentics-communication-graph.json";
    link.click();
    URL.revokeObjectURL(url);
  };

  /** Updates the current graph with validation. */
  const updateGraph = (
    updater: (current: CommunicationGraph) => CommunicationGraph,
  ) => {
    applyGraph(updater(cloneCommunicationGraph(graph)));
  };

  const updateDimensions = (key: "agentCount" | "timeSteps", value: number) => {
    updateGraph((current) => ({ ...current, [key]: value }));
  };

  const updateAnimation = (
    key: keyof CommunicationGraph["animation"],
    value: number,
  ) => {
    updateGraph((current) => ({
      ...current,
      animation: { ...current.animation, [key]: value },
    }));
  };

  const handleVisualPointClick = (point: IndexedPoint) => {
    if (selectedLinkStart === null) {
      setSelectedLinkStart(point);
      setImportErrors([]);
      return;
    }

    if (samePoint(selectedLinkStart, point)) {
      setSelectedLinkStart(null);
      return;
    }

    const [fromAgent, fromTime] = selectedLinkStart;
    const [toAgent, toTime] = point;
    applyGraph({
      ...cloneCommunicationGraph(graph),
      links: [
        ...graph.links,
        [
          [fromAgent, fromTime],
          [toAgent, toTime],
        ],
      ],
    });
  };

  const scheduleVisualPointClick = (point: IndexedPoint) => {
    clearVisualClickTimer();
    visualClickTimerRef.current = setTimeout(() => {
      visualClickTimerRef.current = null;
      handleVisualPointClick(point);
    }, 180);
  };

  const handleVisualDiscoveryToggle = (point: IndexedPoint) => {
    clearVisualClickTimer();
    setSelectedLinkStart(null);
    toggleDiscovery(point);
  };

  const toggleDiscovery = (point: IndexedPoint) => {
    applyGraph({
      ...cloneCommunicationGraph(graph),
      discoveries: hasPoint(graph.discoveries, point)
        ? graph.discoveries.filter((candidate) => !samePoint(candidate, point))
        : [...graph.discoveries, point],
    });
  };

  const clearGraph = () => {
    applyGraph({
      ...cloneCommunicationGraph(graph),
      links: [],
      discoveries: [],
    });
  };

  const deselectVisualPoint = () => {
    clearVisualClickTimer();
    setSelectedLinkStart(null);
  };

  const isPlaying = editorMode === "presentation";

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <div className={styles.titleBlock}>
          <h1 className={styles.title}>Communication Graph Editor</h1>
          <p className={styles.subtitle}>
            Build the timeline graph visually, validate it, import or export
            JSON, and preview the Agentics communication animation with derived
            causal timing.
          </p>
        </div>
      </header>

      <section className={styles.panel} aria-label="Animation workspace">
        <div className={styles.buttonRow}>
          <button
            className={styles.button}
            type="button"
            onClick={() => fileInputRef.current?.click()}
          >
            <Upload size={16} />
            Import
          </button>
          <input
            ref={fileInputRef}
            className={styles.fileInput}
            type="file"
            accept="application/json,.json"
            onChange={handleImport}
          />
          <button
            className={`${styles.button} ${styles.buttonPrimary}`}
            type="button"
            onClick={handleExport}
          >
            <Download size={16} />
            Export
          </button>
          <button className={styles.button} type="button" onClick={clearGraph}>
            <Trash2 size={16} />
            Clear
          </button>
          <button
            className={`${styles.button} ${isPlaying ? styles.playButtonActive : ""}`}
            type="button"
            onClick={() =>
              setEditorMode((mode) =>
                mode === "edit" ? "presentation" : "edit",
              )
            }
            title={isPlaying ? "Stop presentation" : "Play presentation"}
          >
            {isPlaying ? <Square size={16} /> : <Play size={16} />}
            {isPlaying ? "Stop" : "Play"}
          </button>
        </div>

        <div
          className={`${styles.previewFrame} ${
            isPlaying ? styles.presentationFrame : ""
          }`}
          onPointerDown={isPlaying ? undefined : deselectVisualPoint}
        >
          {isPlaying ? (
            <CommunicationTimelineGraph
              title="Communication graph presentation"
              graph={graph}
              className={styles.previewGraph}
            />
          ) : (
            <CommunicationGraphVisualEditor
              graph={graph}
              selectedPoint={selectedLinkStart}
              onDiscoveryToggle={handleVisualDiscoveryToggle}
              onPointClick={scheduleVisualPointClick}
            />
          )}
        </div>

        <div className={styles.controls}>
          <div className={styles.controlGrid}>
            <NumberField
              label="Number of Agents"
              min={1}
              value={graph.agentCount}
              onChange={(value) => updateDimensions("agentCount", value)}
            />
            <NumberField
              label="Time steps"
              min={1}
              value={graph.timeSteps}
              onChange={(value) => updateDimensions("timeSteps", value)}
            />
            <NumberField
              label="Step Duration"
              min={0.01}
              step={0.01}
              value={graph.animation.t}
              onChange={(value) => updateAnimation("t", value)}
            />
            <NumberField
              label="Glow Duration"
              min={0.01}
              step={0.01}
              value={graph.animation.t_glow}
              onChange={(value) => updateAnimation("t_glow", value)}
            />
            <NumberField
              label="Glow Hold"
              min={0.01}
              step={0.01}
              value={graph.animation.t_last}
              onChange={(value) => updateAnimation("t_last", value)}
            />
            <NumberField
              label="Fade-out Duration"
              min={0.01}
              step={0.01}
              value={graph.animation.t_fadeout}
              onChange={(value) => updateAnimation("t_fadeout", value)}
            />
          </div>
        </div>
      </section>
      {importErrors.length > 0 ? (
        <ImportErrorDialog
          errors={importErrors}
          onClose={() => setImportErrors([])}
        />
      ) : null}
    </div>
  );
}

function CommunicationGraphVisualEditor({
  graph,
  onDiscoveryToggle,
  onPointClick,
  selectedPoint,
}: {
  graph: CommunicationGraph;
  onDiscoveryToggle: (point: IndexedPoint) => void;
  onPointClick: (point: IndexedPoint) => void;
  selectedPoint: IndexedPoint | null;
}) {
  const model = deriveCommunicationTimeline(graph);
  const discoveryKeys = new Set(graph.discoveries.map(pointKey));
  const connectedKeys = new Set(
    graph.links.flatMap(([from, to]) => [pointKey(from), pointKey(to)]),
  );

  return (
    <svg
      className={`${styles.previewGraph} ${styles.visualEditorGraph}`}
      viewBox={`0 0 ${model.width} ${model.height}`}
      aria-label="Interactive communication graph editor"
    >
      <title>Interactive communication graph editor</title>
      <rect
        className={styles.editorCanvasHitArea}
        x="0"
        y="0"
        width={model.width}
        height={model.height}
      />
      <g>
        {model.ys.map((y, index) => (
          <g key={`row-${y}`}>
            <path
              className={styles.editorGridLine}
              d={`M${model.xs[0]} ${y} H${model.xs.at(-1)}`}
            />
            <text className={styles.editorAgentLabel} x="44" y={y + 4}>
              {index}
            </text>
          </g>
        ))}
        {model.xs.map((x, index) => (
          <g key={`col-${x}`}>
            <path
              className={styles.editorGridLine}
              d={`M${x} ${model.ys[0]} V${model.ys.at(-1)}`}
            />
            <text
              className={styles.editorTimeLabel}
              x={x - 8}
              y={model.timeLabelY}
            >
              t{index + 1}
            </text>
          </g>
        ))}
      </g>

      <g>
        {model.links.map((link) => (
          <path
            key={link.id}
            className={`${styles.editorLink} ${
              link.kind === "fade" ? styles.editorLinkFade : ""
            }`}
            d={`M${link.from.x} ${link.from.y} L${link.to.x} ${link.to.y}`}
          />
        ))}
      </g>

      <g>
        {model.nodes.map((node) => {
          const key = pointKey(node.index);
          const isDiscovery = discoveryKeys.has(key);
          const isConnected = connectedKeys.has(key);
          const isSelected =
            selectedPoint !== null && samePoint(selectedPoint, node.index);
          const nodeClass = [
            styles.editorNode,
            isConnected ? styles.editorNodeConnected : styles.editorNodeMuted,
            isDiscovery ? styles.editorNodeDiscovery : "",
            isSelected ? styles.editorNodeSelected : "",
          ]
            .filter(Boolean)
            .join(" ");

          return (
            // biome-ignore lint/a11y/useSemanticElements: SVG node hotspots cannot be represented as native HTML buttons without breaking the graph coordinate system.
            <g
              key={key}
              className={styles.editorNodeGroup}
              role="button"
              tabIndex={0}
              aria-pressed={isSelected}
              aria-label={`Node agent ${node.index[0]}, t${node.index[1]}. Double click or right click to toggle discovery.`}
              onPointerDown={(event) => {
                event.stopPropagation();
              }}
              onClick={(event) => {
                event.stopPropagation();
                onPointClick(node.index);
              }}
              onContextMenu={(event) => {
                event.preventDefault();
                event.stopPropagation();
                onDiscoveryToggle(node.index);
              }}
              onDoubleClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                onDiscoveryToggle(node.index);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  event.stopPropagation();
                  onPointClick(node.index);
                }
              }}
            >
              {isSelected ? (
                <circle
                  className={styles.editorSelectedHalo}
                  cx={node.x}
                  cy={node.y}
                  r="25"
                />
              ) : null}
              {isDiscovery ? (
                <circle
                  className={styles.editorDiscoveryHalo}
                  cx={node.x}
                  cy={node.y}
                  r="26"
                />
              ) : null}
              <circle className={nodeClass} cx={node.x} cy={node.y} r="12" />
              <circle
                className={styles.editorHitTarget}
                cx={node.x}
                cy={node.y}
                r="24"
              />
            </g>
          );
        })}
      </g>
    </svg>
  );
}

function ImportErrorDialog({
  errors,
  onClose,
}: {
  errors: string[];
  onClose: () => void;
}) {
  return (
    <div className={styles.dialogBackdrop}>
      <div
        className={styles.dialog}
        role="alertdialog"
        aria-labelledby="import-error-title"
        aria-describedby="import-error-details"
      >
        <div className={styles.dialogHeader}>
          <h2 id="import-error-title" className={styles.dialogTitle}>
            Import failed
          </h2>
          <button
            className={styles.dialogCloseButton}
            type="button"
            onClick={onClose}
            aria-label="Close import errors"
          >
            <X size={16} />
          </button>
        </div>
        <p className={styles.dialogDescription}>
          The current graph was not replaced. Fix these issues and import the
          JSON again.
        </p>
        <ul id="import-error-details" className={styles.dialogList}>
          {errors.map((error, index) => (
            <li key={`${index}-${error}`}>{error}</li>
          ))}
        </ul>
        <button
          className={`${styles.button} ${styles.buttonPrimary}`}
          type="button"
          onClick={onClose}
        >
          OK
        </button>
      </div>
    </div>
  );
}

function pointKey([agent, time]: IndexedPoint) {
  return `${agent}:${time}`;
}

function samePoint(left: IndexedPoint, right: IndexedPoint) {
  return left[0] === right[0] && left[1] === right[1];
}

function hasPoint(points: IndexedPoint[], point: IndexedPoint) {
  return points.some((candidate) => samePoint(candidate, point));
}

function NumberField({
  label,
  min,
  onChange,
  step,
  value,
}: {
  label: string;
  min: number;
  onChange: (value: number) => void;
  step?: number;
  value: number;
}) {
  return (
    <label className={styles.field}>
      {label}
      <input
        type="number"
        min={min}
        step={step ?? 1}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </label>
  );
}
