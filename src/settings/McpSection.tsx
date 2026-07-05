/**
 * McpSection — Model Context Protocol server management.
 *
 * Lets the user register MCP servers (Slack, GitHub, Filesystem, etc.)
 * so their tools appear in the LLM tool registry as first-class calls.
 *
 * Add form: slug + label + URL + optional bearer token.
 * List: enabled toggle + delete per row.
 * Ping button verifies the URL responds with tools/list.
 *
 * Tools discovered from MCP servers are namespaced `mcp_<slug>_<tool>`
 * to avoid collisions across servers.
 */
import { useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  mcpAddServer,
  mcpDeleteServer,
  mcpListServers,
  mcpPingServer,
  mcpSetEnabled,
  type McpServer,
} from "../lib/cloud";

export function McpSection() {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [loading, setLoading] = useState(true);
  const [slug, setSlug] = useState("");
  const [label, setLabel] = useState("");
  const [url, setUrl] = useState("");
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [flash, setFlash] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const list = await mcpListServers();
      setServers(list);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function handleAdd(e: React.FormEvent) {
    e.preventDefault();
    if (!slug.trim() || !label.trim() || !url.trim() || busy) return;
    setBusy(true);
    setError(null);
    setFlash(null);
    try {
      // Ping first so we fail fast on bad URLs.
      const tools = await mcpPingServer(url.trim(), token.trim() || undefined);
      await mcpAddServer(
        slug.trim().toLowerCase().replace(/[^a-z0-9_-]/g, "-"),
        label.trim(),
        url.trim(),
        token.trim() || undefined,
      );
      setFlash(`Added ${label.trim()} — ${tools.length} tool${tools.length === 1 ? "" : "s"} discovered`);
      setSlug("");
      setLabel("");
      setUrl("");
      setToken("");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleToggle(s: McpServer) {
    setError(null);
    try {
      await mcpSetEnabled(s.id, !s.enabled);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleDelete(s: McpServer) {
    setError(null);
    try {
      await mcpDeleteServer(s.id);
      setFlash(`Removed ${s.label}`);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <p className="text-bone-3 text-[11px] leading-relaxed">
        MCP servers expose tools Travis can invoke — Slack, GitHub,
        filesystem, whatever. Tools show up in the LLM registry as
        <code className="mx-1 font-mono">mcp_&lt;slug&gt;_&lt;tool&gt;</code>.
        Tokens stay on this device — never synced to cloud.
        Changes take effect at next Travis turn.
      </p>

      {/* Add form */}
      <form onSubmit={handleAdd} className="flex flex-col gap-2">
        <div className="grid grid-cols-[100px_1fr] gap-2">
          <input
            type="text"
            placeholder="slug"
            value={slug}
            onChange={(e) => setSlug(e.target.value)}
            disabled={busy}
            className="bg-white/[0.02] border rounded-md px-2 py-1.5 text-[12.5px] text-white placeholder:text-white/40 focus:outline-none focus:border-[#bd9eff]/40 disabled:opacity-50 font-mono"
            style={{ borderColor: "rgba(255, 255, 255, 0.1)" }}
          />
          <input
            type="text"
            placeholder="label (e.g. 'Slack')"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            disabled={busy}
            className="bg-white/[0.02] border rounded-md px-2 py-1.5 text-[12.5px] text-white placeholder:text-white/40 focus:outline-none focus:border-[#bd9eff]/40 disabled:opacity-50"
            style={{ borderColor: "rgba(255, 255, 255, 0.1)" }}
          />
        </div>
        <input
          type="url"
          placeholder="https://mcp.example.com/rpc"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          disabled={busy}
          className="bg-white/[0.02] border rounded-md px-2 py-1.5 text-[12.5px] text-white placeholder:text-white/40 focus:outline-none focus:border-[#bd9eff]/40 disabled:opacity-50"
          style={{ borderColor: "rgba(255, 255, 255, 0.1)" }}
        />
        <div className="flex gap-2">
          <input
            type="password"
            placeholder="Bearer token (optional)"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            disabled={busy}
            className="flex-1 bg-white/[0.02] border rounded-md px-2 py-1.5 text-[12.5px] text-white placeholder:text-white/40 focus:outline-none focus:border-[#bd9eff]/40 disabled:opacity-50 font-mono"
            style={{ borderColor: "rgba(255, 255, 255, 0.08)" }}
          />
          <motion.button
            type="submit"
            disabled={busy || !slug.trim() || !label.trim() || !url.trim()}
            whileHover={{ scale: 1.02 }}
            whileTap={{ scale: 0.98 }}
            className="shrink-0 text-[11px] uppercase tracking-wider font-mono px-3 py-1.5 rounded-md transition-colors disabled:opacity-40"
            style={{
              background: "rgba(189, 158, 255, 0.12)",
              color: "rgb(189, 158, 255)",
              border: "1px solid rgba(189, 158, 255, 0.4)",
            }}
          >
            {busy ? "adding…" : "add + verify"}
          </motion.button>
        </div>
      </form>

      <AnimatePresence>
        {flash && (
          <motion.div
            key="flash"
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
            className="text-[11px] font-mono"
            style={{ color: "rgb(129, 199, 132)" }}
          >
            {flash}
          </motion.div>
        )}
        {error && (
          <motion.div
            key="error"
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
            className="text-[11px] font-mono"
            style={{ color: "rgba(255, 130, 130, 0.9)" }}
          >
            {error}
          </motion.div>
        )}
      </AnimatePresence>

      {/* Server list */}
      <div className="flex flex-col gap-1.5">
        {servers.map((s) => (
          <div
            key={s.id}
            className="flex items-center justify-between gap-3 px-2.5 py-1.5 rounded-md border"
            style={{
              borderColor: s.enabled
                ? "rgba(189, 158, 255, 0.28)"
                : "rgba(255, 255, 255, 0.08)",
              background: s.enabled
                ? "rgba(189, 158, 255, 0.04)"
                : "rgba(255, 255, 255, 0.02)",
              opacity: s.enabled ? 1 : 0.6,
            }}
          >
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span
                  className="text-[9px] uppercase tracking-wider font-mono"
                  style={{ color: "rgba(189, 158, 255, 0.8)" }}
                >
                  {s.slug}
                </span>
                <span className="text-[12.5px]" style={{ color: "rgba(236, 236, 241, 0.9)" }}>
                  {s.label}
                </span>
              </div>
              <div
                className="text-[10px] font-mono opacity-60 truncate"
                style={{ color: "rgba(236, 236, 241, 0.7)" }}
              >
                {s.url}
              </div>
            </div>
            <div className="flex items-center gap-1.5 shrink-0">
              <motion.button
                whileHover={{ scale: 1.04 }}
                whileTap={{ scale: 0.96 }}
                onClick={() => handleToggle(s)}
                className="text-[10px] uppercase tracking-wider font-mono px-2 py-1 rounded"
                style={{
                  background: s.enabled
                    ? "rgba(129, 199, 132, 0.15)"
                    : "rgba(255, 255, 255, 0.04)",
                  color: s.enabled ? "rgb(129, 199, 132)" : "rgba(236, 236, 241, 0.6)",
                  border: `1px solid ${
                    s.enabled ? "rgba(129, 199, 132, 0.4)" : "rgba(255, 255, 255, 0.1)"
                  }`,
                }}
              >
                {s.enabled ? "enabled" : "off"}
              </motion.button>
              <motion.button
                whileHover={{ scale: 1.04 }}
                whileTap={{ scale: 0.96 }}
                onClick={() => handleDelete(s)}
                className="text-[10px] uppercase tracking-wider font-mono px-2 py-1 rounded"
                style={{
                  background: "rgba(255, 255, 255, 0.03)",
                  color: "rgba(255, 180, 180, 0.85)",
                  border: "1px solid rgba(255, 100, 100, 0.25)",
                }}
              >
                delete
              </motion.button>
            </div>
          </div>
        ))}
        {!loading && servers.length === 0 && (
          <div className="text-bone-3 text-[11px] font-mono opacity-60">
            No MCP servers configured. Add one above to expose its tools to Travis.
          </div>
        )}
      </div>
    </div>
  );
}
