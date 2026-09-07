import { useState, useEffect, useRef } from 'react';
import { 
  X, 
  Activity, 
  Play, 
  Square, 
  RefreshCw, 
  Trash2, 
  Terminal, 
  ArrowUpRight, 
  ArrowDownLeft, 
  Check, 
  Copy, 
  AlertCircle,
  ExternalLink,
  Code
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface McpTrafficFrame {
  id: string;
  server_name: string;
  direction: 'inbound' | 'outbound';
  method: string | null;
  payload: string;
  timestamp: string;
}

export interface NodeEnvironment {
  available: boolean;
  node_path: string | null;
  node_version: string | null;
  npx_path: string | null;
  guidance: string | null;
}

export interface McpInspectorStatus {
  running: boolean;
  pid: number | null;
  server_name: string | null;
  port: number | null;
}

interface McpInspectorModalProps {
  isOpen: boolean;
  serverName: string;
  serverCommand: string;
  onClose: () => void;
}

export function McpInspectorModal({ isOpen, serverName, serverCommand, onClose }: McpInspectorModalProps) {
  const [frames, setFrames] = useState<McpTrafficFrame[]>([]);
  const [loading, setLoading] = useState(false);
  const [nodeEnv, setNodeEnv] = useState<NodeEnvironment | null>(null);
  const [inspectorStatus, setInspectorStatus] = useState<McpInspectorStatus | null>(null);
  const [launching, setLaunching] = useState(false);
  const [pinging, setPinging] = useState(false);
  const [selectedFrame, setSelectedFrame] = useState<McpTrafficFrame | null>(null);
  const [copied, setCopied] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [pingResult, setPingResult] = useState<string | null>(null);
  const [autoScroll, setAutoScroll] = useState(true);
  const logEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen || !serverName) return;

    loadHistoricalLogs();
    checkNode();
    checkInspector();

    // Listen for live domain events
    let unlistenFn: (() => void) | null = null;
    listen('domain_event', (eventPayload: any) => {
      try {
        const raw = typeof eventPayload.payload === 'string' ? JSON.parse(eventPayload.payload) : eventPayload.payload;
        const domainEv = raw.event || raw;
        if (domainEv.type === 'McpFrameLogged' && domainEv.data?.frame) {
          const frame: McpTrafficFrame = domainEv.data.frame;
          if (frame.server_name === serverName) {
            setFrames(prev => [...prev, frame]);
          }
        }
      } catch {
        // ignore non-matching events
      }
    }).then(unlisten => {
      unlistenFn = unlisten;
    });

    return () => {
      if (unlistenFn) unlistenFn();
    };
  }, [isOpen, serverName]);

  useEffect(() => {
    if (autoScroll && logEndRef.current) {
      logEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [frames, autoScroll]);

  const loadHistoricalLogs = async () => {
    setLoading(true);
    try {
      const logs = await invoke<McpTrafficFrame[]>('get_mcp_traffic_logs', { serverName, limit: 200 });
      setFrames(logs);
      if (logs.length > 0) {
        setSelectedFrame(logs[logs.length - 1]);
      }
    } catch (err) {
      console.error('Failed to load MCP traffic logs:', err);
    } finally {
      setLoading(false);
    }
  };

  const checkNode = async () => {
    try {
      const env = await invoke<NodeEnvironment>('check_node_environment');
      setNodeEnv(env);
    } catch (err) {
      console.error('Failed to check node environment:', err);
    }
  };

  const checkInspector = async () => {
    try {
      const status = await invoke<McpInspectorStatus>('get_mcp_inspector_status');
      setInspectorStatus(status);
    } catch (err) {
      console.error('Failed to get inspector status:', err);
    }
  };

  const handleSendPing = async () => {
    setPinging(true);
    setPingResult(null);
    try {
      const res = await invoke<string>('send_mcp_ping', { serverName });
      setPingResult(`Ping succeeded: ${res}`);
    } catch (err: any) {
      setPingResult(`Ping failed: ${err}`);
    } finally {
      setPinging(false);
      setTimeout(() => setPingResult(null), 4000);
    }
  };

  const handleClearLogs = async () => {
    try {
      await invoke('clear_mcp_traffic_logs', { serverName });
      setFrames([]);
      setSelectedFrame(null);
    } catch (err) {
      console.error('Failed to clear traffic logs:', err);
    }
  };

  const handleLaunchInspector = async () => {
    setLaunching(true);
    try {
      await invoke('launch_mcp_inspector', { serverName });
      await checkInspector();
    } catch (err) {
      console.error('Failed to launch inspector:', err);
    } finally {
      setLaunching(false);
    }
  };

  const handleStopInspector = async () => {
    try {
      await invoke('stop_mcp_inspector');
      await checkInspector();
    } catch (err) {
      console.error('Failed to stop inspector:', err);
    }
  };

  const handleCopyPayload = (text: string) => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  if (!isOpen) return null;

  const isHttp = serverCommand.startsWith('http://') || serverCommand.startsWith('https://');
  const filteredFrames = frames.filter(f => {
    if (!searchQuery) return true;
    const q = searchQuery.toLowerCase();
    return (
      (f.method && f.method.toLowerCase().includes(q)) ||
      f.payload.toLowerCase().includes(q) ||
      f.direction.toLowerCase().includes(q)
    );
  });

  const isInspectorRunning = inspectorStatus?.running && inspectorStatus.server_name === serverName;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm p-4 animate-in fade-in duration-150">
      <div className="relative w-full max-w-5xl h-[88vh] bg-zinc-950 border border-zinc-800 rounded-xl shadow-2xl flex flex-col overflow-hidden">
        
        {/* Header */}
        <div className="px-6 py-4 border-b border-zinc-800 flex items-center justify-between bg-zinc-900/50">
          <div className="flex items-center space-x-3">
            <div className="p-2 rounded-lg bg-brand-primary/10 text-brand-primary border border-brand-primary/20">
              <Activity size={18} />
            </div>
            <div>
              <div className="flex items-center space-x-2">
                <h2 className="text-base font-bold text-white tracking-tight">MCP Protocol Inspector</h2>
                <span className="text-xs font-mono px-2 py-0.5 rounded bg-zinc-800 text-zinc-300 border border-zinc-700">
                  {serverName}
                </span>
                <span
                  className={`text-[10px] font-mono px-2 py-0.5 rounded border ${
                    isHttp
                      ? 'bg-cyan-950/60 text-cyan-300 border-cyan-800/50'
                      : 'bg-zinc-800/80 text-zinc-400 border-zinc-700/50'
                  }`}
                >
                  {isHttp ? 'Streamable HTTP (SSE)' : 'Stdio Subprocess'}
                </span>
              </div>
              <div className="text-xs font-mono text-zinc-500 mt-0.5 truncate max-w-xl">
                {serverCommand}
              </div>
            </div>
          </div>

          <div className="flex items-center space-x-3">
            {pingResult && (
              <span className={`text-xs font-mono px-2 py-1 rounded ${
                pingResult.includes('succeeded') ? 'bg-emerald-950 text-emerald-300 border border-emerald-800' : 'bg-red-950 text-red-300 border border-red-800'
              }`}>
                {pingResult}
              </span>
            )}
            <button
              onClick={handleSendPing}
              disabled={pinging}
              className="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-200 text-xs font-medium border border-zinc-700 transition disabled:opacity-50"
              title="Dispatches a live ping to generate JSON-RPC traffic"
            >
              <RefreshCw size={13} className={pinging ? 'animate-spin' : ''} />
              <span>{pinging ? 'Pinging...' : 'Send Ping'}</span>
            </button>

            <button
              onClick={onClose}
              className="p-1.5 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800 transition"
            >
              <X size={18} />
            </button>
          </div>
        </div>

        {/* Official Inspector Banner */}
        <div className="px-6 py-2.5 bg-zinc-900/30 border-b border-zinc-800 flex items-center justify-between text-xs">
          <div className="flex items-center space-x-2">
            <Terminal size={14} className="text-zinc-400" />
            <span className="font-semibold text-zinc-300">Official MCP Inspector CLI:</span>
            {nodeEnv?.available ? (
              <span className="text-emerald-400 font-mono text-[11px] flex items-center space-x-1">
                <Check size={12} />
                <span>Node {nodeEnv.node_version || 'Ready'} detected</span>
              </span>
            ) : (
              <span className="text-amber-400 text-[11px] flex items-center space-x-1">
                <AlertCircle size={12} />
                <span>Node.js not installed (pure-Rust protocol log active)</span>
              </span>
            )}
          </div>

          <div className="flex items-center space-x-3">
            {isInspectorRunning ? (
              <div className="flex items-center space-x-2">
                <span className="text-[11px] font-mono text-emerald-400 flex items-center space-x-1">
                  <span className="h-2 w-2 rounded-full bg-emerald-500 animate-pulse"></span>
                  <span>PID: {inspectorStatus.pid} (Port {inspectorStatus.port || 5173})</span>
                </span>
                <button
                  onClick={handleStopInspector}
                  className="flex items-center space-x-1 px-2.5 py-1 rounded bg-red-950/70 hover:bg-red-900 border border-red-800/50 text-red-200 text-xs font-semibold"
                >
                  <Square size={12} />
                  <span>Stop Inspector</span>
                </button>
              </div>
            ) : (
              <button
                onClick={handleLaunchInspector}
                disabled={launching || !nodeEnv?.available}
                className={`flex items-center space-x-1.5 px-3 py-1 rounded text-xs font-semibold border transition ${
                  nodeEnv?.available
                    ? 'bg-cyan-950/60 hover:bg-cyan-900/80 text-cyan-300 border-cyan-700/50'
                    : 'bg-zinc-900 text-zinc-500 border-zinc-800 cursor-not-allowed'
                }`}
                title={nodeEnv?.available ? 'Spawns @modelcontextprotocol/inspector via npx' : nodeEnv?.guidance || ''}
              >
                <Play size={12} className={launching ? 'animate-pulse' : ''} />
                <span>{launching ? 'Launching...' : 'Launch Official Inspector'}</span>
              </button>
            )}
          </div>
        </div>

        {/* Guidance dialog when Node is absent */}
        {nodeEnv && !nodeEnv.available && (
          <div className="px-6 py-2 bg-amber-950/20 border-b border-amber-900/30 flex items-center justify-between text-xs text-amber-300/90">
            <div className="flex items-center space-x-2">
              <AlertCircle size={14} className="text-amber-400 shrink-0" />
              <span>{nodeEnv.guidance}</span>
            </div>
            <a
              href="https://nodejs.org"
              target="_blank"
              rel="noreferrer"
              className="flex items-center space-x-1 text-cyan-400 hover:underline shrink-0 text-[11px]"
            >
              <span>Download Node.js</span>
              <ExternalLink size={11} />
            </a>
          </div>
        )}

        {/* Main Content Area: Split View (Table + Detail) */}
        <div className="flex-1 flex overflow-hidden">
          
          {/* Left: Traffic Frame List */}
          <div className="w-1/2 border-r border-zinc-800 flex flex-col bg-zinc-950">
            {/* Toolbar */}
            <div className="p-3 border-b border-zinc-800 flex items-center justify-between bg-zinc-900/30">
              <input
                type="text"
                placeholder="Filter by method or content..."
                value={searchQuery}
                onChange={e => setSearchQuery(e.target.value)}
                className="w-56 rounded bg-zinc-900 border border-zinc-800 px-2.5 py-1 text-xs text-white placeholder-zinc-500 focus:outline-none focus:border-zinc-700 font-mono"
              />

              <div className="flex items-center space-x-2">
                <label className="flex items-center space-x-1.5 text-[11px] text-zinc-400 cursor-pointer select-none">
                  <input
                    type="checkbox"
                    checked={autoScroll}
                    onChange={e => setAutoScroll(e.target.checked)}
                    className="rounded bg-zinc-800 border-zinc-700 h-3.5 w-3.5"
                  />
                  <span>Auto-scroll</span>
                </label>

                <button
                  onClick={handleClearLogs}
                  className="p-1 rounded text-zinc-500 hover:text-red-400 hover:bg-zinc-800 transition"
                  title="Clear traffic history"
                >
                  <Trash2 size={14} />
                </button>
              </div>
            </div>

            {/* Frame List */}
            <div className="flex-1 overflow-y-auto divide-y divide-zinc-900 font-mono text-xs">
              {loading && frames.length === 0 ? (
                <div className="p-8 text-center text-zinc-500 italic">Loading traffic frames...</div>
              ) : filteredFrames.length === 0 ? (
                <div className="p-8 text-center text-zinc-500 space-y-2">
                  <Code size={24} className="mx-auto opacity-40 text-zinc-400" />
                  <p>No protocol traffic recorded yet.</p>
                  <p className="text-[11px] text-zinc-600">
                    Click &quot;Send Ping&quot; above to generate live test traffic, or execute an agent task using this server.
                  </p>
                </div>
              ) : (
                filteredFrames.map((f, idx) => {
                  const isOut = f.direction === 'outbound';
                  const isSelected = selectedFrame?.id === f.id;
                  const time = new Date(f.timestamp).toLocaleTimeString();

                  return (
                    <div
                      key={f.id}
                      onClick={() => setSelectedFrame(f)}
                      className={`p-2.5 cursor-pointer transition flex items-center justify-between ${
                        isSelected
                          ? 'bg-zinc-800/80 border-l-2 border-brand-primary'
                          : 'hover:bg-zinc-900/50'
                      }`}
                    >
                      <div className="flex items-center space-x-2.5 overflow-hidden">
                        <span className="text-[10px] text-zinc-600 w-6 shrink-0 font-mono">
                          #{idx + 1}
                        </span>

                        <span
                          className={`flex items-center space-x-1 px-1.5 py-0.5 rounded text-[10px] font-bold shrink-0 ${
                            isOut
                              ? 'bg-cyan-950/80 text-cyan-300 border border-cyan-800/40'
                              : 'bg-emerald-950/80 text-emerald-300 border border-emerald-800/40'
                          }`}
                        >
                          {isOut ? <ArrowUpRight size={10} /> : <ArrowDownLeft size={10} />}
                          <span>{isOut ? 'OUT' : 'IN'}</span>
                        </span>

                        <span className="font-semibold text-zinc-200 truncate">
                          {f.method || 'result'}
                        </span>
                      </div>

                      <div className="flex items-center space-x-2 text-[11px] text-zinc-500 shrink-0">
                        <span>{time}</span>
                      </div>
                    </div>
                  );
                })
              )}
              <div ref={logEndRef} />
            </div>
          </div>

          {/* Right: Frame Payload Inspector */}
          <div className="w-1/2 flex flex-col bg-zinc-950">
            <div className="p-3 border-b border-zinc-800 flex items-center justify-between bg-zinc-900/30">
              <span className="text-xs font-semibold text-zinc-300">Frame JSON-RPC Payload</span>
              {selectedFrame && (
                <button
                  onClick={() => handleCopyPayload(selectedFrame.payload)}
                  className="flex items-center space-x-1 px-2.5 py-1 rounded bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-xs transition"
                >
                  {copied ? <Check size={12} className="text-emerald-400" /> : <Copy size={12} />}
                  <span>{copied ? 'Copied' : 'Copy JSON'}</span>
                </button>
              )}
            </div>

            <div className="flex-1 p-4 overflow-y-auto font-mono text-xs text-zinc-300 bg-zinc-950">
              {selectedFrame ? (
                <pre className="whitespace-pre-wrap break-all leading-relaxed">
                  {(() => {
                    try {
                      return JSON.stringify(JSON.parse(selectedFrame.payload), null, 2);
                    } catch {
                      return selectedFrame.payload;
                    }
                  })()}
                </pre>
              ) : (
                <div className="h-full flex items-center justify-center text-zinc-600 italic">
                  Select a traffic frame on the left to inspect its complete JSON-RPC structure.
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Footer */}
        <div className="px-6 py-3 border-t border-zinc-800 bg-zinc-900/50 flex items-center justify-between text-xs text-zinc-500">
          <div>
            Total Frames: <span className="text-zinc-300 font-mono font-bold">{frames.length}</span> (
            <span className="text-cyan-400 font-mono">{frames.filter(f => f.direction === 'outbound').length} OUT</span>,{' '}
            <span className="text-emerald-400 font-mono">{frames.filter(f => f.direction === 'inbound').length} IN</span>)
          </div>
          <button
            onClick={onClose}
            className="px-4 py-1.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-300 font-medium transition"
          >
            Close Inspector
          </button>
        </div>

      </div>
    </div>
  );
}
