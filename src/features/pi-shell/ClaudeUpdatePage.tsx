"use client";

import { useEffect, useState } from "react";
import { GitBranch, RefreshCw, SquareTerminal, Tag } from "lucide-react";

import { GroupRow, InsetGroup, SettingsPage } from "@/components/settings-ui";
import { api } from "@/lib/tauri-api";
import { APP_VERSION, useUpdate } from "@/lib/update";
import type { ClaudeStatus } from "@/features/sessions/types";

export default function ClaudeUpdatePage() {
  const update = useUpdate();
  const [claude, setClaude] = useState<ClaudeStatus>({
    installed: false,
    version: "检测中…",
    executablePath: null,
  });

  useEffect(() => {
    void update.check();
    void api.claudeStatus().then(setClaude);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <SettingsPage
      title="软件更新"
      subtitle="保留 Pi Desktop Software Update 模块，更新对象改为 Claudeix 与 Claude Code 状态。"
    >
      <div style={{ display: "grid", placeItems: "center", padding: "26px 0 18px" }}>
        <div
          style={{
            width: 92,
            height: 92,
            display: "grid",
            placeItems: "center",
            borderRadius: 22,
            background: "var(--accent)",
            color: "white",
            fontSize: 42,
            fontWeight: 800,
            boxShadow: "var(--shadow-md)",
          }}
        >
          C
        </div>
        <div style={{ marginTop: 12, fontSize: 17, fontWeight: 650 }}>Claudeix</div>
        <div style={{ marginTop: 3, color: "var(--text-tertiary)", fontSize: 12 }}>
          v{update.info?.currentVersion ?? APP_VERSION}
        </div>
      </div>

      <InsetGroup header="Claudeix">
        <GroupRow
          first
          icon={<Tag size={15} />}
          title="当前版本"
          detail={update.info?.currentVersion ?? APP_VERSION}
        />
        <GroupRow
          icon={<GitBranch size={15} />}
          title="最新版本"
          detail={
            update.phase === "checking"
              ? "检查中…"
              : update.info?.latestVersion ?? "暂无 Release tag"
          }
        />
        <GroupRow
          icon={<RefreshCw size={15} />}
          title="检查更新"
          detail={
            update.phase === "available"
              ? "发现新版本；自动安装尚未启用，可前往 GitHub Release 获取"
              : update.phase === "upToDate"
                ? "当前已是最新版本"
                : update.error ?? "从 Claudeix GitHub tags 检查"
          }
          onClick={() => void update.check()}
        />
      </InsetGroup>

      <InsetGroup header="Claude Code">
        <GroupRow
          first
          icon={<SquareTerminal size={15} />}
          title={claude.installed ? "Claude Code 已安装" : "未检测到 Claude Code"}
          detail={claude.version}
        />
        <GroupRow
          icon={<SquareTerminal size={15} />}
          title="可执行文件"
          detail={claude.executablePath ?? "—"}
          onClick={() => void api.claudeStatus().then(setClaude)}
        />
      </InsetGroup>

      <p style={{ marginTop: 14, color: "var(--text-tertiary)", fontSize: 11.5, lineHeight: 1.6 }}>
        Claudeix v0.1 暂不启用自动覆盖安装程序；这样可以避免未签名更新链直接替换本地程序。Release 安装包仍由本地/正式发布流程生成。
      </p>
    </SettingsPage>
  );
}
