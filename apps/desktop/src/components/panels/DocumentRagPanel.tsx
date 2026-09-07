import React, { useState, useEffect } from 'react';
import { 
  FileText, 
  Search, 
  RefreshCw, 
  FileCode, 
  Layers, 
  Sparkles, 
  ExternalLink, 
  CheckCircle,
  Clock,
  Filter
} from 'lucide-react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { useUiStore } from '../../store/uiStore';

export interface DocIngestState {
  project_id: string;
  file_path: string;
  content_hash: string;
  chunk_count: number;
  updated_at: string;
}

export interface DocSearchResult {
  chunk_id: string;
  file_path: string;
  line_start: number;
  line_end: number;
  snippet: string;
  kind: string;
  score: number;
}

export default function DocumentRagPanel() {
  const { activeProjectId, setSelectedFile } = useUiStore();
  const [ingestStates, setIngestStates] = useState<DocIngestState[]>([]);
  const [searchResults, setSearchResults] = useState<DocSearchResult[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [filePattern, setFilePattern] = useState('');
  const [limit, setLimit] = useState(10);
  const [isIngesting, setIsIngesting] = useState(false);
  const [isSearching, setIsSearching] = useState(false);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [activeSubTab, setActiveSubTab] = useState<'search' | 'indexed'>('search');

  const loadIngestStates = async () => {
    const isRunningInTauri = typeof window !== 'undefined' && (isTauri() || (window as any).__TAURI_INTERNALS__ !== undefined);
    if (!isRunningInTauri) {
      setIngestStates([
        {
          project_id: activeProjectId || 'proj-trans4mers-local',
          file_path: 'core/trans4mers-engine/src/scheduler.rs',
          content_hash: 'sha256-a9f2c7104b...',
          chunk_count: 8,
          updated_at: new Date().toISOString(),
        },
        {
          project_id: activeProjectId || 'proj-trans4mers-local',
          file_path: 'core/trans4mers-engine/src/swarm_orchestrator.rs',
          content_hash: 'sha256-5b8d21c4e...',
          chunk_count: 14,
          updated_at: new Date().toISOString(),
        },
        {
          project_id: activeProjectId || 'proj-trans4mers-local',
          file_path: 'core/trans4mers-storage/src/repos/memory_repo.rs',
          content_hash: 'sha256-78e11a09f...',
          chunk_count: 12,
          updated_at: new Date().toISOString(),
        },
        {
          project_id: activeProjectId || 'proj-trans4mers-local',
          file_path: 'docs/architecture/MEMORY_AND_RAG_ARCHITECTURE.md',
          content_hash: 'sha256-f402ccb31...',
          chunk_count: 22,
          updated_at: new Date().toISOString(),
        }
      ]);
      setSearchResults([
        {
          chunk_id: 'chk-sch-01',
          file_path: 'core/trans4mers-engine/src/scheduler.rs',
          line_start: 31,
          line_end: 45,
          snippet: 'pub async fn acquire_permit(&self, agent_id: &str) -> Result<OwnedSemaphorePermit, SchedulerError> {\n    let permit = self.node_semaphore.clone().acquire_owned().await...;\n    Ok(permit)\n}',
          kind: 'rust_function',
          score: 0.942,
        },
        {
          chunk_id: 'chk-mem-02',
          file_path: 'core/trans4mers-storage/src/rrf.rs',
          line_start: 12,
          line_end: 28,
          snippet: 'pub fn calculate_rrf_score(rank_fts: usize, rank_vec: usize, k: f64) -> f64 {\n    (1.0 / (k + rank_fts as f64)) + (1.0 / (k + rank_vec as f64))\n}',
          kind: 'rust_function',
          score: 0.887,
        },
      ]);
      setSearchQuery('acquire_permit tokio semaphore');
      return;
    }

    if (!activeProjectId) return;
    try {
      const states = await invoke<DocIngestState[]>('list_ingested_documents', {
        projectId: activeProjectId,
      });
      setIngestStates(states || []);
    } catch (e) {
      console.error('Failed to load ingested documents:', e);
    }
  };

  useEffect(() => {
    loadIngestStates();
  }, [activeProjectId]);

  const handleIngest = async () => {
    if (!activeProjectId) return;
    setIsIngesting(true);
    setStatusMessage('Indexing workspace documents with BM25 & semantic vectors...');
    try {
      const chunkCount = await invoke<number>('ingest_documents', {
        projectId: activeProjectId,
        paths: null,
      });
      setStatusMessage(`Successfully indexed ${chunkCount} chunks across workspace.`);
      await loadIngestStates();
    } catch (e) {
      setStatusMessage(`Indexing failed: ${e}`);
    } finally {
      setIsIngesting(false);
    }
  };

  const handleSearch = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!activeProjectId || !searchQuery.trim()) return;
    setIsSearching(true);
    try {
      const results = await invoke<DocSearchResult[]>('search_documents', {
        projectId: activeProjectId,
        query: searchQuery,
        limit,
        filePattern: filePattern.trim() ? filePattern.trim() : null,
      });
      setSearchResults(results || []);
      setActiveSubTab('search');
    } catch (e) {
      console.error('Search failed:', e);
    } finally {
      setIsSearching(false);
    }
  };

  const totalChunks = ingestStates.reduce((acc, s) => acc + s.chunk_count, 0);

  return (
    <div className="h-full flex flex-col bg-bg-base text-text-primary overflow-hidden font-sans">
      {/* Header bar */}
      <div className="p-4 border-b border-gray-800 bg-bg-surface flex items-center justify-between">
        <div className="flex items-center space-x-3">
          <div className="p-2 rounded-lg bg-brand-primary/10 text-brand-primary border border-brand-primary/20">
            <FileText size={20} />
          </div>
          <div>
            <h2 className="text-sm font-bold text-gray-100 uppercase tracking-wider font-mono">
              Document & Code RAG Substrate
            </h2>
            <div className="text-xs text-gray-500 font-mono flex items-center space-x-2 mt-0.5">
              <span>{ingestStates.length} Files</span>
              <span>•</span>
              <span>{totalChunks} Chunks</span>
              <span>•</span>
              <span className="text-brand-primary/80">Hybrid BM25 + sqlite-vec</span>
            </div>
          </div>
        </div>

        <div className="flex items-center space-x-2">
          <button
            onClick={handleIngest}
            disabled={isIngesting || !activeProjectId}
            className={`px-3 py-1.5 rounded text-xs font-semibold flex items-center space-x-1.5 border transition-all ${
              isIngesting
                ? 'bg-gray-800 text-gray-500 border-gray-700 cursor-not-allowed'
                : 'bg-brand-primary/10 hover:bg-brand-primary/20 text-brand-primary border-brand-primary/30'
            }`}
          >
            <RefreshCw size={13} className={isIngesting ? 'animate-spin' : ''} />
            <span>{isIngesting ? 'Indexing...' : 'Index Workspace'}</span>
          </button>
        </div>
      </div>

      {statusMessage && (
        <div className="px-4 py-2 bg-gray-900 border-b border-gray-800 text-xs font-mono text-gray-300 flex items-center justify-between">
          <div className="flex items-center space-x-2">
            <CheckCircle size={14} className="text-emerald-400 shrink-0" />
            <span>{statusMessage}</span>
          </div>
          <button
            onClick={() => setStatusMessage(null)}
            className="text-gray-500 hover:text-gray-300 text-xs"
          >
            Dismiss
          </button>
        </div>
      )}

      {/* Search & Navigation Bar */}
      <div className="p-4 bg-bg-surface/50 border-b border-gray-800 space-y-3">
        <form onSubmit={handleSearch} className="flex gap-2">
          <div className="relative flex-1">
            <Search size={14} className="absolute left-3 top-2.5 text-gray-500" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search code symbols, documentation, or conceptual queries..."
              className="w-full bg-bg-base border border-gray-800 rounded pl-9 pr-3 py-1.5 text-xs text-gray-200 placeholder-gray-500 focus:outline-none focus:border-brand-primary/50 font-mono"
            />
          </div>

          <div className="relative w-44">
            <Filter size={14} className="absolute left-3 top-2.5 text-gray-500" />
            <input
              type="text"
              value={filePattern}
              onChange={(e) => setFilePattern(e.target.value)}
              placeholder="Filter by path (.rs, src/)"
              className="w-full bg-bg-base border border-gray-800 rounded pl-9 pr-3 py-1.5 text-xs text-gray-200 placeholder-gray-500 focus:outline-none focus:border-brand-primary/50 font-mono"
            />
          </div>

          <select
            value={limit}
            onChange={(e) => setLimit(Number(e.target.value))}
            className="bg-bg-base border border-gray-800 rounded px-2 py-1.5 text-xs text-gray-300 font-mono focus:outline-none"
          >
            <option value={5}>Top 5</option>
            <option value={10}>Top 10</option>
            <option value={20}>Top 20</option>
          </select>

          <button
            type="submit"
            disabled={isSearching || !searchQuery.trim()}
            className="px-4 py-1.5 bg-brand-primary text-black font-semibold text-xs rounded hover:bg-brand-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed font-mono flex items-center space-x-1"
          >
            {isSearching ? <RefreshCw size={12} className="animate-spin" /> : <Sparkles size={12} />}
            <span>Search</span>
          </button>
        </form>

        {/* Sub-tabs */}
        <div className="flex items-center space-x-2 text-xs font-mono">
          <button
            onClick={() => setActiveSubTab('search')}
            className={`px-2.5 py-1 rounded transition-colors ${
              activeSubTab === 'search'
                ? 'bg-gray-800 text-gray-100 font-semibold'
                : 'text-gray-400 hover:text-gray-200'
            }`}
          >
            Search Results ({searchResults.length})
          </button>
          <button
            onClick={() => setActiveSubTab('indexed')}
            className={`px-2.5 py-1 rounded transition-colors ${
              activeSubTab === 'indexed'
                ? 'bg-gray-800 text-gray-100 font-semibold'
                : 'text-gray-400 hover:text-gray-200'
            }`}
          >
            Indexed Files ({ingestStates.length})
          </button>
        </div>
      </div>

      {/* Main Content Area */}
      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {activeSubTab === 'search' ? (
          searchResults.length === 0 ? (
            <div className="h-64 flex flex-col items-center justify-center text-gray-500 space-y-2">
              <Search size={32} className="text-gray-700" />
              <p className="text-xs font-mono">
                {searchQuery ? 'No chunks matched query' : 'Enter a query to run hybrid semantic + keyword search'}
              </p>
            </div>
          ) : (
            searchResults.map((result, idx) => (
              <div
                key={result.chunk_id}
                className="bg-bg-surface border border-gray-800 hover:border-gray-700 rounded-lg p-3 transition-colors space-y-2"
              >
                <div className="flex items-center justify-between text-xs font-mono">
                  <div className="flex items-center space-x-2 truncate">
                    <span className="px-1.5 py-0.5 rounded bg-brand-primary/10 text-brand-primary font-bold text-[10px]">
                      #{idx + 1}
                    </span>
                    {result.kind === 'code' ? (
                      <FileCode size={13} className="text-blue-400 shrink-0" />
                    ) : (
                      <FileText size={13} className="text-amber-400 shrink-0" />
                    )}
                    <span className="font-semibold text-gray-200 truncate">{result.file_path}</span>
                    <span className="text-gray-500">
                      :L{result.line_start}-L{result.line_end}
                    </span>
                  </div>

                  <div className="flex items-center space-x-2">
                    <span className="text-[10px] text-gray-500">
                      RRF: {result.score.toFixed(4)}
                    </span>
                    <button
                      onClick={() => setSelectedFile(result.file_path)}
                      className="p-1 rounded hover:bg-gray-800 text-gray-400 hover:text-gray-200 transition-colors"
                      title="Open file in editor"
                    >
                      <ExternalLink size={13} />
                    </button>
                  </div>
                </div>

                <pre className="p-2.5 rounded bg-bg-base border border-gray-900 text-xs font-mono text-gray-300 overflow-x-auto whitespace-pre-wrap leading-relaxed max-h-48">
                  {result.snippet}
                </pre>
              </div>
            ))
          )
        ) : (
          <div className="space-y-2">
            {ingestStates.length === 0 ? (
              <div className="h-64 flex flex-col items-center justify-center text-gray-500 space-y-2">
                <Layers size={32} className="text-gray-700" />
                <p className="text-xs font-mono">No documents indexed yet. Click "Index Workspace" above.</p>
              </div>
            ) : (
              ingestStates.map((state) => (
                <div
                  key={state.file_path}
                  className="bg-bg-surface border border-gray-800 p-3 rounded-lg flex items-center justify-between text-xs font-mono"
                >
                  <div className="flex items-center space-x-2.5 truncate">
                    <FileCode size={14} className="text-brand-primary shrink-0" />
                    <span className="text-gray-200 font-medium truncate">{state.file_path}</span>
                  </div>
                  <div className="flex items-center space-x-4 text-gray-500 shrink-0">
                    <span>{state.chunk_count} chunks</span>
                    <div className="flex items-center space-x-1">
                      <Clock size={12} />
                      <span>{new Date(state.updated_at).toLocaleDateString()}</span>
                    </div>
                    <button
                      onClick={() => setSelectedFile(state.file_path)}
                      className="p-1 rounded hover:bg-gray-800 text-gray-400 hover:text-gray-200 transition-colors"
                      title="Open file"
                    >
                      <ExternalLink size={13} />
                    </button>
                  </div>
                </div>
              ))
            )}
          </div>
        )}
      </div>
    </div>
  );
}
