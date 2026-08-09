"use client";

import { useEffect, useMemo, useState } from "react";
import { Braces, MonitorCog, RefreshCw, Save, Shield, Terminal } from "lucide-react";

import {
  ACCENT_PRESETS,
  BG_PRESETS,
  FONT_SCALES,
  TEXT_PRESETS,
  useAppearance,
} from "@/lib/appearance";
import { api, type ClaudeSettingsFile } from "@/lib/tauri-api";
import { useUI, type CloseBehavior, type Theme } from "@/lib/store";
import { useWorkspace } from "@/lib/workspace";
import {
  CodeArea,
  ColorSwatches,
  GroupRow,
  InsetGroup,
  IOSSwitch,
  Segmented,
  SettingsPage,
  SliderRow,
  TextRow,
} from "@/components/settings-ui";

type Scope = "global" | "project";
type SettingsObject = Record<string, unknown>;

function parseSettings(file: ClaudeSettingsFile | null): SettingsObject {
  if (!file) return {};
  try {
    const parsed = JSON.parse(file.content) as unknown;
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as SettingsObject)
      : {};
  } catch {
    return {};
  }
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

export default function ClaudeSettingsPage() {
  const workspace = useWorkspace((state) => state.root);
  const theme = useUI((state) => state.theme);
  const closeBehavior = useUI((state) => state.closeBehavior);
  const notifications = useUI((state) => state.notificationSettings.enabled);
  const appearance = useAppearance();
  const [scope, setScope] = useState<Scope>("global");
  const [file, setFile] = useState<ClaudeSettingsFile | null>(null);
  const [rawDraft, setRawDraft] = useState("{}\n");
  const [loading, setLoading] = useState(false);
  const [savingRaw, setSavingRaw] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const settings = useMemo(() => parseSettings(file), [file]);
  const env = useMemo(() => {
    const value = settings.env;
    return value && typeof value === "object" && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : {};
  }, [settings]);

  const load = async (nextScope = scope) => {
    setLoading(true);
    setError(null);
    try {
      const result = await api.readClaudeSettings(nextScope, workspace);
      setFile(result);
      setRawDraft(result.content);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load(scope);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scope, workspace]);

  const writeObject = async (next: SettingsObject) => {
    setError(null);
    try {
      const content = `${JSON.stringify(next, null, 2)}\n`;
      const result = await api.writeClaudeSettings(scope, content, workspace);
      setFile(result);
      setRawDraft(result.content);
    } catch (cause) {
      setError(String(cause));
    }
  };

  const setKey = (key: string, value: unknown) => {
    const next = { ...settings };
    if (value === undefined) delete next[key];
    else next[key] = value;
    void writeObject(next);
  };

  const setEnv = (key: string, value: string | undefined) => {
    const nextEnv = { ...env };
    if (value === undefined) delete nextEnv[key];
    else nextEnv[key] = value;
    const next = { ...settings };
    if (Object.keys(nextEnv).length === 0) delete next.env;
    else next.env = nextEnv;
    void writeObject(next);
  };

  const saveRaw = async () => {
    setSavingRaw(true);
    setError(null);
    try {
      const result = await api.writeClaudeSettings(scope, rawDraft, workspace);
      setFile(result);
      setRawDraft(result.content);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setSavingRaw(false);
    }
  };

  return (
    <SettingsPage
      title="设置"
      subtitle="Pi Desktop 的设置界面结构，配置对象已切换为 Claude Code 与 Claudeix。"
    >
      <InsetGroup header="Claude Code 配置范围" footer={file?.path ?? "正在读取 settings.json…"}>
        <div style={{ padding: "12px 14px" }}>
          <Segmented
            options={["global", "project"] as const}
            value={scope}
            onChange={setScope}
            labelOf={(value) => (value === "global" ? "全局" : "当前项目")}
          />
        </div>
        <GroupRow
          icon={<RefreshCw size={15} />}
          title={loading ? "正在读取…" : "重新读取 Claude settings.json"}
          detail={scope === "global" ? "~/.claude/settings.json" : "<workspace>/.claude/settings.json"}
          onClick={() => void load()}
        />
      </InsetGroup>

      <InsetGroup header="Claude Code" footer="这些键直接写入 Claude Code 原生 settings.json；未知字段会完整保留。">
        <TextRow
          first
          label="默认模型"
          detail="Claude Code settings.json → model"
          value={stringValue(settings.model)}
          placeholder="例如 opus / sonnet / 完整模型名"
          onCommit={(value) => setKey("model", value)}
        />
        <TextRow
          label="界面语言"
          detail="Claude Code settings.json → language"
          value={stringValue(settings.language)}
          placeholder="例如 zh-CN / en-US"
          onCommit={(value) => setKey("language", value)}
        />
        <TextRow
          label="Claude Code 主题"
          detail="Claude Code settings.json → theme"
          value={stringValue(settings.theme)}
          placeholder="留空使用 Claude Code 默认值"
          onCommit={(value) => setKey("theme", value)}
        />
        <GroupRow
          icon={<Shield size={15} />}
          title="提交信息包含 Co-Authored-By"
          detail="Claude Code settings.json → includeCoAuthoredBy"
          trailing={
            <IOSSwitch
              checked={settings.includeCoAuthoredBy !== false}
              onChange={(checked) => setKey("includeCoAuthoredBy", checked)}
            />
          }
        />
      </InsetGroup>

      <InsetGroup
        header="Provider / API 环境"
        footer="保留 Pi Desktop 的 Provider 配置思路，但映射为 Claude Code 使用的 env。密钥不在普通表单中回显。"
      >
        <TextRow
          first
          label="ANTHROPIC_BASE_URL"
          detail="自定义 Claude/兼容渠道 Base URL"
          value={stringValue(env.ANTHROPIC_BASE_URL)}
          placeholder="https://api.anthropic.com"
          onCommit={(value) => setEnv("ANTHROPIC_BASE_URL", value)}
        />
        <TextRow
          label="ANTHROPIC_MODEL"
          detail="可选环境级模型覆盖；通常优先使用上面的 model 设置"
          value={stringValue(env.ANTHROPIC_MODEL)}
          placeholder="留空"
          onCommit={(value) => setEnv("ANTHROPIC_MODEL", value)}
        />
        <GroupRow
          icon={<Terminal size={15} />}
          title="API Key / Auth Token"
          detail="为避免在 UI 明文回显密钥，请在下方原始 JSON 中编辑 env，或由系统环境变量管理。"
        />
      </InsetGroup>

      <InsetGroup header="Claudeix 外观" footer="这些设置只影响 Claudeix，不修改 Claude Code。">
        <GroupRow
          first
          icon={<MonitorCog size={15} />}
          title="明暗主题"
          trailing={
            <div style={{ width: 170 }}>
              <Segmented
                options={["dark", "light"] as const}
                value={theme}
                onChange={(value: Theme) => {
                  if (value !== useUI.getState().theme) useUI.getState().toggleTheme();
                }}
                labelOf={(value) => (value === "dark" ? "深色" : "浅色")}
              />
            </div>
          }
        />
        <div style={{ padding: "12px 16px", borderTop: "1px solid var(--separator)" }}>
          <div style={{ marginBottom: 10, fontSize: 13.5 }}>强调色</div>
          <ColorSwatches
            value={appearance.accent}
            presets={ACCENT_PRESETS}
            defaultLabel="默认强调色"
            customLabel="自定义强调色"
            onChange={(accent) => appearance.set({ accent })}
          />
        </div>
        <div style={{ padding: "12px 16px", borderTop: "1px solid var(--separator)" }}>
          <div style={{ marginBottom: 10, fontSize: 13.5 }}>背景色</div>
          <ColorSwatches
            value={appearance.background}
            presets={BG_PRESETS}
            defaultLabel="默认背景"
            customLabel="自定义背景"
            onChange={(background) => appearance.set({ background })}
          />
        </div>
        <div style={{ padding: "12px 16px", borderTop: "1px solid var(--separator)" }}>
          <div style={{ marginBottom: 10, fontSize: 13.5 }}>文字颜色</div>
          <ColorSwatches
            value={appearance.textColor}
            presets={TEXT_PRESETS}
            defaultLabel="默认文字颜色"
            customLabel="自定义文字颜色"
            onChange={(textColor) => appearance.set({ textColor })}
          />
        </div>
        <div style={{ padding: "5px 16px 13px", borderTop: "1px solid var(--separator)" }}>
          <SliderRow
            label="界面缩放"
            value={appearance.fontScale}
            min={FONT_SCALES[0]}
            max={FONT_SCALES[FONT_SCALES.length - 1]}
            step={0.05}
            format={(value) => `${Math.round(value * 100)}%`}
            onChange={(fontScale) => appearance.set({ fontScale })}
          />
        </div>
      </InsetGroup>

      <InsetGroup header="窗口与通知">
        <GroupRow
          first
          icon={<MonitorCog size={15} />}
          title="关闭窗口行为"
          trailing={
            <div style={{ width: 260 }}>
              <Segmented
                options={["ask", "minimize", "quit"] as const}
                value={closeBehavior}
                onChange={(value: CloseBehavior) => useUI.getState().setCloseBehavior(value)}
                labelOf={(value) =>
                  value === "ask" ? "询问" : value === "minimize" ? "最小化" : "退出"
                }
              />
            </div>
          }
        />
        <GroupRow
          icon={<Shield size={15} />}
          title="桌面通知"
          trailing={
            <IOSSwitch
              checked={notifications}
              onChange={(checked) => useUI.getState().setNotificationEnabled(checked)}
            />
          }
        />
      </InsetGroup>

      <InsetGroup header="Custom CSS" footer="继续使用 Pi Desktop 原有的客户端外观覆盖机制。">
        <div style={{ padding: 14 }}>
          <CodeArea
            value={appearance.customCss}
            onChange={(customCss) => appearance.set({ customCss })}
            placeholder="/* Claudeix custom CSS */"
            rows={7}
          />
        </div>
      </InsetGroup>

      <InsetGroup
        header="Claude settings.json · 高级"
        footer="保存前会验证 JSON；现有 settings.json 会备份为 settings.json.claudeix.bak。"
      >
        <div style={{ padding: 14 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 10 }}>
            <Braces size={15} />
            <span style={{ fontSize: 13 }}>原始 JSON</span>
            <button
              onClick={() => void saveRaw()}
              disabled={savingRaw}
              style={{
                marginLeft: "auto",
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
                border: "none",
                borderRadius: 8,
                padding: "6px 10px",
                background: "var(--accent)",
                color: "white",
                cursor: savingRaw ? "default" : "pointer",
                opacity: savingRaw ? 0.6 : 1,
              }}
            >
              <Save size={13} /> {savingRaw ? "保存中…" : "保存"}
            </button>
          </div>
          <CodeArea value={rawDraft} onChange={setRawDraft} rows={14} />
        </div>
      </InsetGroup>

      {error && (
        <p style={{ marginTop: 14, fontSize: 12, color: "var(--danger)" }}>{error}</p>
      )}

      <div style={{ height: 30 }} />
    </SettingsPage>
  );
}
