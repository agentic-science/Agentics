"use client";

import { useState } from "react";

interface FileItem {
  path: string;
  size: number;
  is_text: boolean;
  content?: string | null;
}

/**
 * Render a solution submission archive as a sorted file list with a text preview pane.
 *
 * Binary or oversized files are represented by metadata only, matching the
 * backend artifact DTO's `is_text` and optional `content` fields.
 */
export function CodeBrowser({ files }: { files: FileItem[] }) {
  const sorted = [...files].sort((a, b) => a.path.localeCompare(b.path));
  const [activePath, setActivePath] = useState<string | null>(
    sorted.find((f) => f.is_text)?.path ?? null,
  );

  const activeFile = sorted.find((f) => f.path === activePath);

  return (
    <div className="code-browser">
      <div className="file-list">
        {sorted.map((file) => (
          <button
            key={file.path}
            type="button"
            className={`file-button${activePath === file.path ? " active" : ""}`}
            onClick={() => setActivePath(file.path)}
          >
            <strong>{file.path}</strong>
            <span>{file.size.toLocaleString()} bytes</span>
          </button>
        ))}
      </div>
      <div className="code-view">
        {activeFile ? (
          <>
            <div className="code-meta">
              <span>{activeFile.path}</span>
              <span>{activeFile.size.toLocaleString()} bytes</span>
            </div>
            {activeFile.is_text && activeFile.content ? (
              <pre>{activeFile.content}</pre>
            ) : (
              <div className="empty-block">该文件为二进制或内容过大。</div>
            )}
          </>
        ) : (
          <div className="empty-block">选择一个文件以查看内容</div>
        )}
      </div>
    </div>
  );
}
