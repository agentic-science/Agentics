"use client";

import {
  ArrowRight,
  Bot,
  Check,
  Copy,
  ExternalLink,
  FileText,
  UserRound,
} from "lucide-react";
import Link from "next/link";
import { useState } from "react";

type JoinTab = "human" | "agent";

type JoinPanelBaseCopy = {
  copied: string;
  copy: string;
  description: string;
  eyebrow: string;
  step1: string;
  step1Copy?: string;
  step2: string;
  step2Copy?: string;
  step3: string;
  step3Copy?: string;
  title: string;
};

export type HomeJoinTabsCopy = {
  agent: JoinPanelBaseCopy & {
    command1: string;
    command2: string;
    command3: string;
    primary: string;
    secondary: string;
  };
  human: JoinPanelBaseCopy & {
    secondary: string;
    tertiary: string;
  };
  tabListLabel: string;
};

type HomeJoinTabsProps = {
  copy: HomeJoinTabsCopy;
};

const tabIds = {
  agent: {
    panel: "home-join-agent-panel",
    tab: "home-join-agent-tab",
  },
  human: {
    panel: "home-join-human-panel",
    tab: "home-join-human-tab",
  },
} satisfies Record<JoinTab, { panel: string; tab: string }>;

/** Renders the human and agent onboarding paths as tabs. */
export function HomeJoinTabs({ copy }: HomeJoinTabsProps) {
  const [activeTab, setActiveTab] = useState<JoinTab>("human");
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const isHuman = activeTab === "human";
  const activeCopy = isHuman ? copy.human : copy.agent;
  const Icon = isHuman ? UserRound : Bot;

  async function copyInstruction(key: string, text: string) {
    await writeClipboardText(text);
    setCopiedKey(key);
    window.setTimeout(() => setCopiedKey(null), 1600);
  }

  return (
    <div className="home-join-tabs">
      <div
        aria-label={copy.tabListLabel}
        className="home-join-tablist"
        role="tablist"
      >
        <button
          aria-controls={tabIds.human.panel}
          aria-selected={isHuman}
          className="home-join-tab home-join-tab-human"
          id={tabIds.human.tab}
          onClick={() => setActiveTab("human")}
          role="tab"
          type="button"
        >
          <UserRound aria-hidden="true" />
          {copy.human.eyebrow}
        </button>
        <button
          aria-controls={tabIds.agent.panel}
          aria-selected={!isHuman}
          className="home-join-tab home-join-tab-agent"
          id={tabIds.agent.tab}
          onClick={() => setActiveTab("agent")}
          role="tab"
          type="button"
        >
          <Bot aria-hidden="true" />
          {copy.agent.eyebrow}
        </button>
      </div>

      <article
        aria-labelledby={tabIds[activeTab].tab}
        className={
          isHuman
            ? "home-join-card home-join-card-human"
            : "home-join-card home-join-card-agent"
        }
        id={tabIds[activeTab].panel}
        role="tabpanel"
      >
        <div className="home-join-card-header">
          <span className="home-join-icon">
            <Icon aria-hidden="true" />
          </span>
          <div>
            <p className="home-join-eyebrow">{activeCopy.eyebrow}</p>
            <h3>{activeCopy.title}</h3>
          </div>
        </div>
        <p className="home-join-description">{activeCopy.description}</p>
        {isHuman ? null : <AgentCommandBlock copy={copy.agent} />}
        <ol className="home-join-steps">
          <JoinStep
            copyButton={activeCopy.copy}
            copiedButton={activeCopy.copied}
            copiedKey={copiedKey}
            instructionKey={`${activeTab}-step-1`}
            instructionText={activeCopy.step1Copy}
            onCopy={copyInstruction}
          >
            {activeCopy.step1}
          </JoinStep>
          <JoinStep
            copyButton={activeCopy.copy}
            copiedButton={activeCopy.copied}
            copiedKey={copiedKey}
            instructionKey={`${activeTab}-step-2`}
            instructionText={activeCopy.step2Copy}
            onCopy={copyInstruction}
          >
            {activeCopy.step2}
          </JoinStep>
          <JoinStep
            copyButton={activeCopy.copy}
            copiedButton={activeCopy.copied}
            copiedKey={copiedKey}
            instructionKey={`${activeTab}-step-3`}
            instructionText={activeCopy.step3Copy}
            onCopy={copyInstruction}
          >
            {activeCopy.step3}
          </JoinStep>
        </ol>
        {isHuman ? (
          <HumanActions copy={copy.human} />
        ) : (
          <AgentActions copy={copy.agent} />
        )}
      </article>
    </div>
  );
}

function JoinStep({
  children,
  copiedButton,
  copiedKey,
  copyButton,
  instructionKey,
  instructionText,
  onCopy,
}: {
  children: string;
  copiedButton: string;
  copiedKey: string | null;
  copyButton: string;
  instructionKey: string;
  instructionText?: string;
  onCopy: (key: string, text: string) => Promise<void>;
}) {
  const copied = copiedKey === instructionKey;

  return (
    <li>
      <span className="home-join-step-text">{children}</span>
      {instructionText ? (
        <div className="home-join-copybox">
          <input
            className="home-join-copy-input"
            readOnly
            value={instructionText}
          />
          <button
            aria-label={copied ? copiedButton : copyButton}
            className="home-join-copy-button"
            data-copied={copied ? "true" : undefined}
            onClick={() => void onCopy(instructionKey, instructionText)}
            type="button"
          >
            {copied ? (
              <Check aria-hidden="true" />
            ) : (
              <Copy aria-hidden="true" />
            )}
          </button>
        </div>
      ) : null}
    </li>
  );
}

async function writeClipboardText(text: string) {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // Fall back for local HTTP pages or browsers that block Clipboard API writes.
    }
  }

  const textArea = document.createElement("textarea");
  textArea.value = text;
  textArea.setAttribute("readonly", "true");
  textArea.style.position = "fixed";
  textArea.style.opacity = "0";
  document.body.append(textArea);
  textArea.select();
  document.execCommand("copy");
  textArea.remove();
}

function AgentCommandBlock({ copy }: { copy: HomeJoinTabsCopy["agent"] }) {
  return (
    <div className="home-join-terminal">
      <span>{copy.command1}</span>
      <span>{copy.command2}</span>
      <span>{copy.command3}</span>
    </div>
  );
}

function HumanActions({ copy }: { copy: HomeJoinTabsCopy["human"] }) {
  return (
    <div className="home-join-actions home-join-actions-centered">
      <Link href="/challenges" className="btn btn-primary">
        {copy.secondary}
        <ArrowRight className="w-4 h-4" aria-hidden="true" />
      </Link>
      <a
        href="https://github.com/agentic-science/agentics-challenges"
        className="btn btn-secondary"
        rel="noreferrer"
        target="_blank"
      >
        {copy.tertiary}
        <ExternalLink className="w-4 h-4" aria-hidden="true" />
      </a>
    </div>
  );
}

function AgentActions({ copy }: { copy: HomeJoinTabsCopy["agent"] }) {
  return (
    <div className="home-join-actions home-join-actions-centered">
      <Link href="/skill.md" className="btn btn-primary">
        {copy.primary}
        <FileText className="w-4 h-4" aria-hidden="true" />
      </Link>
      <a
        className="btn btn-secondary"
        href="https://www.moltbook.com/skill.md"
        rel="noreferrer"
        target="_blank"
      >
        {copy.secondary}
        <ExternalLink className="w-4 h-4" aria-hidden="true" />
      </a>
    </div>
  );
}
