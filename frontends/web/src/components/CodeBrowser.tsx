"use client";

import { Check, Copy, File, FileCode, FileJson, FileText } from "lucide-react";
import { useState } from "react";

interface FileItem {
  path: string;
  size: number;
  is_text: boolean;
  content?: string | null;
  highlightedHtml?: string | null;
}

function getFileIcon(path: string) {
  if (path.endsWith(".py"))
    return <FileCode className="w-4 h-4 text-[var(--accent-secondary-400)]" />;
  if (path.endsWith(".json"))
    return <FileJson className="w-4 h-4 text-[var(--accent-primary-400)]" />;
  if (path.endsWith(".md") || path.endsWith(".txt"))
    return <FileText className="w-4 h-4 text-[var(--text-muted)]" />;
  return <File className="w-4 h-4 text-[var(--text-muted)]" />;
}

function getLanguage(path: string): string {
  if (path.endsWith(".py")) return "python";
  if (path.endsWith(".json")) return "json";
  if (path.endsWith(".md")) return "markdown";
  if (path.endsWith(".sh")) return "bash";
  if (path.endsWith(".js") || path.endsWith(".ts")) return "typescript";
  if (path.endsWith(".yaml") || path.endsWith(".yml")) return "yaml";
  if (path.endsWith(".toml")) return "toml";
  return "text";
}

export function CodeBrowser({ files }: { files: FileItem[] }) {
  const sorted = [...files].sort((a, b) => a.path.localeCompare(b.path));
  const [activePath, setActivePath] = useState<string | null>(
    sorted.find((f) => f.is_text)?.path ?? null,
  );
  const [copied, setCopied] = useState(false);

  const activeFile = sorted.find((f) => f.path === activePath);

  const handleCopy = async () => {
    if (!activeFile?.content) return;
    await navigator.clipboard.writeText(activeFile.content);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="grid grid-cols-1 md:grid-cols-[240px_1fr] gap-4">
      {/* File Tree */}
      <div className="flex flex-col gap-1 max-h-[600px] overflow-y-auto pr-1">
        {sorted.map((file) => (
          <button
            key={file.path}
            type="button"
            onClick={() => setActivePath(file.path)}
            className={`flex items-center gap-2 px-3 py-2 rounded-lg text-left text-[var(--text-body-sm)] transition-colors ${
              activePath === file.path
                ? "bg-[var(--accent-primary-500)]/10 text-[var(--accent-primary-400)]"
                : "text-[var(--text-muted)] hover:bg-[var(--surface-secondary)] hover:text-[var(--text-primary)]"
            }`}
          >
            {getFileIcon(file.path)}
            <span className="truncate flex-1 font-mono text-xs">
              {file.path}
            </span>
          </button>
        ))}
      </div>

      {/* Code View */}
      <div className="code-view min-w-0">
        {activeFile ? (
          <>
            <div className="code-view-header">
              <div className="flex items-center gap-2">
                {getFileIcon(activeFile.path)}
                <span>{activeFile.path}</span>
                <span className="text-[var(--text-muted)]">
                  {activeFile.size.toLocaleString()} bytes
                </span>
              </div>
              {activeFile.is_text && activeFile.content && (
                <button
                  type="button"
                  onClick={handleCopy}
                  className="btn btn-ghost btn-sm"
                  title="Copy"
                >
                  {copied ? (
                    <Check className="w-3.5 h-3.5" />
                  ) : (
                    <Copy className="w-3.5 h-3.5" />
                  )}
                </button>
              )}
            </div>
            {activeFile.is_text && activeFile.highlightedHtml ? (
              <div
                className="p-4 overflow-x-auto"
                dangerouslySetInnerHTML={{ __html: activeFile.highlightedHtml }}
              />
            ) : activeFile.is_text && activeFile.content ? (
              <pre className="p-4 overflow-x-auto">{activeFile.content}</pre>
            ) : (
              <div className="empty-state py-12">
                <p className="text-[var(--text-muted)]">
                  Binary or oversized file
                </p>
              </div>
            )}
          </>
        ) : (
          <div className="empty-state py-12">
            <p className="text-[var(--text-muted)]">Select a file to view</p>
          </div>
        )}
      </div>
    </div>
  );
}

export { getFileIcon, getLanguage };
