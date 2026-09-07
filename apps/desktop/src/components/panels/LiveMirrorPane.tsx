import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { 
  Globe, 
  ArrowLeft, 
  ArrowRight, 
  RotateCw, 
  Lock, 
  Camera, 
  Plus, 
  AlertCircle 
} from 'lucide-react';
import { useUiStore } from '../../store/uiStore';

interface BrowserSpace {
  id: string;
  name: string;
  profile_path: string;
  is_active: boolean;
  created_at: string;
}

interface LiveFrame {
  space_id: string;
  current_url: string;
  page_title: string;
  status_code: number;
  is_secure: boolean;
  html_snippet: string;
  last_action: string;
  timestamp: string;
}

interface BrowserSnapshot {
  id: string;
  space_id: string;
  reason?: string;
  tree_hash: string;
  file_count: number;
  total_size_bytes: number;
  triggered_by?: string;
  created_at: string;
}

export default function LiveMirrorPane() {
  const { activeProjectId } = useUiStore();

  const [spaces, setSpaces] = useState<BrowserSpace[]>([]);
  const [selectedSpaceId, setSelectedSpaceId] = useState<string>('');
  const [urlInput, setUrlInput] = useState('');
  const [currentFrame, setCurrentFrame] = useState<LiveFrame | null>(null);
  const [snapshots, setSnapshots] = useState<BrowserSnapshot[]>([]);
  const [loading, setLoading] = useState(false);
  const [navHistory, setNavHistory] = useState<string[]>([]);
  const [navIndex, setNavIndex] = useState<number>(0);
  const [newSpaceName, setNewSpaceName] = useState('');
  const [isCreatingSpace, setIsCreatingSpace] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load spaces
  const loadSpaces = async () => {
    if (!activeProjectId) return;
    try {
      let sps = await invoke<BrowserSpace[]>('get_browser_spaces', { projectId: activeProjectId });
      if (sps.length === 0) {
        try {
          const primarySpace = await invoke<BrowserSpace>('create_browser_space', {
            projectId: activeProjectId,
            name: 'Primary Session',
          });
          sps = [primarySpace];
        } catch {
          // If creation fails, keep empty
        }
      }
      setSpaces(sps);
      if (sps.length > 0 && !selectedSpaceId) {
        setSelectedSpaceId(sps[0].id);
      }
    } catch (err) {
      console.error('Failed to load browser spaces:', err);
    }
  };

  // Load snapshots for selected space
  const loadSnapshots = async (spaceId: string) => {
    if (!activeProjectId || !spaceId) return;
    try {
      const snaps = await invoke<BrowserSnapshot[]>('list_browser_snapshots', {
        projectId: activeProjectId,
        spaceId,
      });
      setSnapshots(snaps);
    } catch (err) {
      console.error('Failed to load snapshots:', err);
    }
  };

  useEffect(() => {
    loadSpaces();
  }, [activeProjectId]);

  useEffect(() => {
    if (selectedSpaceId) {
      loadSnapshots(selectedSpaceId);
    }

    if (!activeProjectId) return;
    const unlisten = listen<string>('domain_event', (event) => {
      try {
        const payload = typeof event.payload === 'string' ? JSON.parse(event.payload) : event.payload;
        const ev = payload?.event || payload;
        const evType = ev?.type || ev?.event_type;
        const toolName = ev?.data?.tool_name || ev?.tool_name;
        if (evType === 'ToolExecuted' && typeof toolName === 'string' && toolName.includes('browser')) {
          if (selectedSpaceId) {
            loadSnapshots(selectedSpaceId);
          }
        }
      } catch (e) {
        console.warn('Failed to parse domain_event in LiveMirrorPane:', e);
      }
    });

    return () => {
      unlisten.then(f => f());
    };
  }, [selectedSpaceId, activeProjectId]);

  const handleCreateSpace = async () => {
    if (!activeProjectId || !newSpaceName.trim()) return;
    try {
      const newSpace = await invoke<BrowserSpace>('create_browser_space', {
        projectId: activeProjectId,
        name: newSpaceName.trim(),
      });
      setSpaces((prev) => [...prev, newSpace]);
      setSelectedSpaceId(newSpace.id);
      setNewSpaceName('');
      setIsCreatingSpace(false);
    } catch (err: any) {
      setError(typeof err === 'string' ? err : err.message || 'Failed to create browser space');
    }
  };

  const handleNavigate = async (targetUrl?: string, addToHistory: boolean = true) => {
    const dest = targetUrl || urlInput;
    if (!activeProjectId || !dest.trim()) return;
    setLoading(true);
    setError(null);

    const spaceId = selectedSpaceId || (spaces[0]?.id) || 'bspace_default';

    try {
      const frame = await invoke<LiveFrame>('browser_navigate', {
        projectId: activeProjectId,
        spaceId,
        url: dest.trim(),
      });
      setCurrentFrame(frame);
      setUrlInput(frame.current_url);
      if (addToHistory) {
        setNavHistory(prev => [...prev.slice(0, navIndex + 1), frame.current_url]);
        setNavIndex(prev => prev + 1);
      }
      await loadSnapshots(spaceId);
    } catch (err: any) {
      setError(typeof err === 'string' ? err : err.message || 'Navigation failed');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="h-full flex flex-col bg-bg-base font-mono select-none overflow-hidden">
      {/* Top Header & Space Selector */}
      <div className="h-12 border-b border-gray-800 bg-bg-surface px-4 flex items-center justify-between shrink-0">
        <div className="flex items-center space-x-3">
          <div className="p-1.5 rounded bg-cyan-500/10 text-cyan-400 border border-cyan-500/20">
            <Globe size={16} />
          </div>
          <div className="flex items-center space-x-2">
            <span className="text-xs font-bold text-gray-200 uppercase tracking-wider">Per-Space Live-Mirror</span>
            <span className="text-[10px] px-1.5 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 font-normal">
              SANDBOX FETCH
            </span>
          </div>
        </div>

        <div className="flex items-center space-x-2">
          <label className="text-[11px] text-gray-400">Space:</label>
          <select
            value={selectedSpaceId}
            onChange={(e) => setSelectedSpaceId(e.target.value)}
            className="bg-black/40 border border-gray-700 rounded px-2 py-1 text-xs text-gray-200 focus:outline-none focus:border-cyan-500"
          >
            {spaces.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name} ({s.id.slice(0, 10)}...)
              </option>
            ))}
            {spaces.length === 0 && <option value="bspace_default">Default Isolation Space</option>}
          </select>

          <button
            onClick={() => setIsCreatingSpace(!isCreatingSpace)}
            className="p-1 rounded hover:bg-gray-800 text-gray-400 hover:text-gray-200 border border-gray-700"
            title="Create new isolated browser space"
          >
            <Plus size={14} />
          </button>
        </div>
      </div>

      {/* New Space Modal/Drawer if opened */}
      {isCreatingSpace && (
        <div className="px-4 py-2 bg-gray-900/90 border-b border-gray-800 flex items-center space-x-2 text-xs">
          <input
            type="text"
            placeholder="Space Name (e.g. Research Sandbox, Auth Session)"
            value={newSpaceName}
            onChange={(e) => setNewSpaceName(e.target.value)}
            className="bg-black/50 border border-gray-700 rounded px-3 py-1 text-gray-100 flex-1 focus:outline-none focus:border-cyan-500"
          />
          <button
            onClick={handleCreateSpace}
            disabled={!newSpaceName.trim()}
            className="px-3 py-1 bg-cyan-600 hover:bg-cyan-500 text-white rounded font-medium disabled:opacity-50 transition-colors"
          >
            Create
          </button>
          <button
            onClick={() => setIsCreatingSpace(false)}
            className="px-3 py-1 text-gray-400 hover:text-gray-200"
          >
            Cancel
          </button>
        </div>
      )}

      {/* Browser Navigation Bar */}
      <div className="h-10 border-b border-gray-800 bg-black/40 px-4 flex items-center space-x-2 shrink-0">
        <div className="flex items-center space-x-1 text-gray-400">
          <button 
            onClick={() => {
              if (navIndex > 0) {
                const newIdx = navIndex - 1;
                setNavIndex(newIdx);
                handleNavigate(navHistory[newIdx], false);
              }
            }}
            disabled={navIndex <= 0 || loading}
            className="p-1 rounded hover:bg-gray-800 hover:text-gray-200 disabled:opacity-30 disabled:hover:bg-transparent"
            title="Go Back"
          >
            <ArrowLeft size={14} />
          </button>
          <button 
            onClick={() => {
              if (navIndex < navHistory.length - 1) {
                const newIdx = navIndex + 1;
                setNavIndex(newIdx);
                handleNavigate(navHistory[newIdx], false);
              }
            }}
            disabled={navIndex >= navHistory.length - 1 || loading}
            className="p-1 rounded hover:bg-gray-800 hover:text-gray-200 disabled:opacity-30 disabled:hover:bg-transparent"
            title="Go Forward"
          >
            <ArrowRight size={14} />
          </button>
          <button 
            onClick={() => handleNavigate(undefined, false)}
            className="p-1 rounded hover:bg-gray-800 hover:text-gray-200"
            title="Reload Page"
          >
            <RotateCw size={14} className={loading ? 'animate-spin' : ''} />
          </button>
        </div>

        {/* URL Input */}
        <div className="flex-1 flex items-center bg-bg-surface border border-gray-800 rounded px-2.5 py-1 text-xs">
          <Lock size={12} className="text-emerald-400 mr-2 shrink-0" />
          <input
            type="text"
            value={urlInput}
            onChange={(e) => setUrlInput(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleNavigate()}
            placeholder="Enter URL to navigate agent..."
            className="bg-transparent text-gray-200 w-full focus:outline-none font-mono text-xs placeholder-gray-600"
          />
        </div>

        <button
          onClick={() => handleNavigate()}
          disabled={loading}
          className="px-3 py-1 bg-gray-800 hover:bg-gray-700 text-gray-200 border border-gray-700 rounded text-xs font-semibold flex items-center space-x-1 transition-colors disabled:opacity-50"
        >
          <span>Go</span>
        </button>
      </div>

      {/* Error notification */}
      {error && (
        <div className="px-4 py-2 bg-rose-500/10 border-b border-rose-500/20 text-rose-400 text-xs flex items-center space-x-2">
          <AlertCircle size={14} />
          <span>{error}</span>
        </div>
      )}

      {/* Main Viewport & Snapshots Sidebar */}
      <div className="flex-1 flex overflow-hidden">
        {/* Live Viewport Area */}
        <div className="flex-1 flex flex-col bg-[#121214] p-4 overflow-hidden">
          {/* Viewport Frame Container */}
          <div className="flex-1 rounded-lg border border-gray-800 bg-black/60 flex flex-col overflow-hidden relative shadow-inner">
            {/* Viewport Status Header */}
            <div className="h-7 border-b border-gray-800/80 bg-gray-900/60 px-3 flex items-center justify-between text-[11px] text-gray-400">
              <div className="flex items-center space-x-2 truncate">
                <span className="w-2 h-2 rounded-full bg-emerald-400" />
                <span className="font-bold text-gray-200 truncate">
                  {currentFrame ? currentFrame.page_title : 'Isolated Agent Browser Session'}
                </span>
              </div>
              <div className="flex items-center space-x-3 text-[10px] text-gray-500">
                <span>{currentFrame ? `HTTP ${currentFrame.status_code}` : 'Ready'}</span>
                <span>Zero-Trust Sandbox</span>
              </div>
            </div>

            {/* Viewport Body */}
            <div className="flex-1 p-6 flex flex-col justify-center items-center text-center overflow-auto">
              {currentFrame ? (
                <div className="w-full max-w-2xl text-left bg-bg-surface border border-gray-800 rounded-lg p-6 font-sans">
                  <div className="flex items-center justify-between pb-4 border-b border-gray-800 mb-4">
                    <h2 className="text-lg font-bold text-gray-100 font-mono">{currentFrame.page_title}</h2>
                    <span className="text-xs px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 font-mono">
                      200 OK
                    </span>
                  </div>
                  <iframe 
                    sandbox="allow-same-origin"
                    srcDoc={currentFrame.html_snippet}
                    className="w-full h-80 border-0 rounded bg-black/40 text-gray-200"
                    title={currentFrame.page_title}
                  />
                  <div className="mt-6 pt-4 border-t border-gray-800/80 flex items-center justify-between text-xs text-gray-500 font-mono">
                    <span>Last Agent Action: {currentFrame.last_action}</span>
                    <span>Captured: {new Date(currentFrame.timestamp).toLocaleTimeString()}</span>
                  </div>
                </div>
              ) : (
                <div className="text-gray-500 flex flex-col items-center">
                  <Globe size={48} className="mb-3 opacity-30 text-cyan-400" />
                  <p className="text-sm font-semibold text-gray-300">Live Mirror Viewport</p>
                  <p className="text-xs text-gray-500 mt-1 max-w-sm">
                    Enter a URL above or let an autonomous agent navigate the web. Real-time isolated DOM snapshots and parsed web text mirror live here.
                  </p>
                  <button
                    onClick={() => handleNavigate('https://docs.rs')}
                    className="mt-4 px-3 py-1.5 rounded bg-cyan-500/10 hover:bg-cyan-500/20 text-cyan-300 border border-cyan-500/30 text-xs transition-colors"
                  >
                    Navigate to docs.rs
                  </button>
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Snapshots & Provenance Drawer (Right Side) */}
        <div className="w-72 border-l border-gray-800 bg-bg-surface flex flex-col shrink-0">
          <div className="p-3 border-b border-gray-800 flex items-center justify-between text-xs font-semibold uppercase tracking-wider text-gray-400">
            <div className="flex items-center space-x-1.5">
              <Camera size={14} />
              <span>Snapshots ({snapshots.length})</span>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-2 space-y-2">
            {snapshots.map((snap) => (
              <div
                key={snap.id}
                className="p-2.5 rounded bg-black/40 border border-gray-800 hover:border-gray-700 transition-colors text-xs flex flex-col space-y-1"
              >
                <div className="flex items-center justify-between">
                  <span className="font-bold text-gray-300 truncate text-[11px]">
                    {snap.reason || 'Page Snapshot'}
                  </span>
                  <span className="text-[10px] text-gray-500">
                    {new Date(snap.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                  </span>
                </div>
                <div className="text-[10px] text-gray-500 font-mono truncate">
                  Hash: {snap.tree_hash.slice(0, 16)}...
                </div>
                <div className="flex items-center justify-between text-[10px] text-gray-400 pt-1">
                  <span>Size: {snap.total_size_bytes} bytes</span>
                  <span className="text-cyan-400">{snap.triggered_by || 'agent'}</span>
                </div>
              </div>
            ))}

            {snapshots.length === 0 && (
              <div className="py-8 text-center text-gray-600 text-xs">
                No snapshots captured yet.
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
