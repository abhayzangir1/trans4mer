import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { 
  Compass, 
  BookOpen, 
  FileText, 
  CheckCircle2, 
  Copy, 
  Check, 
  AlertCircle,
  Sparkles,
  ArrowRight
} from 'lucide-react';
import { useUiStore } from '../../store/uiStore';
import { useConversationStore } from '../../store/conversationStore';

interface Citation {
  index: number;
  title: string;
  url_or_path: string;
  snippet: string;
}

interface ResearchReport {
  topic: string;
  executive_summary: string;
  key_findings: string[];
  citations: Citation[];
  markdown_content: string;
}

export default function DeepResearchPane() {
  const { activeProjectId } = useUiStore();
  const { activeConversationId } = useConversationStore();

  const [topic, setTopic] = useState('');
  const [loading, setLoading] = useState(false);
  const [currentStage, setCurrentStage] = useState<number>(0);
  const [report, setReport] = useState<ResearchReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const stages = [
    { title: 'Plan', desc: 'Decompose core inquiries' },
    { title: 'Search', desc: 'Query substrate & memory' },
    { title: 'Extract', desc: 'Extract factual statements' },
    { title: 'Synthesize', desc: 'Consolidate cross-references' },
    { title: 'Cite & Verify', desc: 'Ground with strict citations' },
  ];

  const handleStartResearch = async () => {
    if (!topic.trim() || !activeProjectId) return;
    setLoading(true);
    setError(null);
    setReport(null);
    setCurrentStage(1);

    try {
      const convoId = activeConversationId || 'conv_default';
      const result = await invoke<ResearchReport>('start_deep_research', {
        projectId: activeProjectId,
        conversationId: convoId,
        topic: topic.trim(),
      });
      setCurrentStage(5);
      setReport(result);
    } catch (err: any) {
      setCurrentStage(0);
      setError(typeof err === 'string' ? err : err.message || 'Research execution failed');
    } finally {
      setLoading(false);
    }
  };

  const handleCopy = () => {
    if (!report) return;
    navigator.clipboard.writeText(report.markdown_content);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="h-full flex flex-col bg-bg-base overflow-y-auto p-6 font-mono select-none">
      {/* Header */}
      <div className="flex items-center justify-between pb-6 border-b border-gray-800">
        <div className="flex items-center space-x-3">
          <div className="p-2.5 rounded-lg bg-indigo-500/10 text-indigo-400 border border-indigo-500/20">
            <Compass size={22} />
          </div>
          <div>
            <h1 className="text-xl font-bold text-gray-100 flex items-center space-x-2">
              <span>Cited Deep-Research Engine</span>
              <span className="text-xs px-2 py-0.5 rounded-full bg-indigo-500/10 text-indigo-400 border border-indigo-500/20 font-normal">
                5-STAGE VERIFIABLE
              </span>
            </h1>
            <p className="text-xs text-gray-500 mt-0.5">
              Plan → Search → Read/Extract → Synthesize → Grounded Citations with persistent markdown artifacts.
            </p>
          </div>
        </div>
      </div>

      {/* Inquiry Input Bar */}
      <div className="my-6 p-4 rounded-lg bg-bg-surface border border-gray-800">
        <label className="text-xs font-semibold text-gray-400 uppercase tracking-wider block mb-2">
          Research Topic / Domain Inquiry
        </label>
        <div className="flex items-center space-x-3">
          <input
            type="text"
            placeholder="e.g. Multi-agent consensus protocols, zero-trust SQLite encryption..."
            value={topic}
            onChange={(e) => setTopic(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && !loading && handleStartResearch()}
            disabled={loading}
            className="flex-1 bg-black/40 border border-gray-700 rounded px-3 py-2 text-sm text-gray-100 placeholder-gray-600 focus:outline-none focus:border-indigo-500"
          />
          <button
            onClick={handleStartResearch}
            disabled={loading || !topic.trim() || !activeProjectId}
            className="px-5 py-2 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-medium text-xs flex items-center space-x-2 transition-colors disabled:opacity-50"
          >
            <Sparkles size={14} className={loading ? 'animate-spin' : ''} />
            <span>{loading ? 'Researching...' : 'Conduct Research'}</span>
          </button>
        </div>
      </div>

      {/* 5-Stage Stepper */}
      <div className="grid grid-cols-5 gap-3 mb-6">
        {stages.map((stage, idx) => {
          const stepNum = idx + 1;
          const isDone = Boolean(report && currentStage >= 5);
          const isCurrent = loading;

          return (
            <div
              key={stage.title}
              className={`p-3 rounded-lg border transition-colors ${
                isDone
                  ? 'bg-emerald-500/5 border-emerald-500/20 text-emerald-400'
                  : isCurrent
                  ? 'bg-indigo-500/10 border-indigo-500/30 text-indigo-400 animate-pulse'
                  : 'bg-bg-surface border-gray-800 text-gray-500'
              }`}
            >
              <div className="flex items-center justify-between">
                <span className="text-[10px] font-bold uppercase tracking-wider">
                  Stage {stepNum}
                </span>
                {isDone ? (
                  <CheckCircle2 size={13} className="text-emerald-400" />
                ) : (
                  <span className="text-[10px] font-mono">0{stepNum}</span>
                )}
              </div>
              <div className="text-xs font-bold mt-1 text-gray-200">{stage.title}</div>
              <div className="text-[10px] text-gray-500 mt-0.5 truncate">{stage.desc}</div>
            </div>
          );
        })}
      </div>

      {/* Error Banner */}
      {error && (
        <div className="p-4 rounded-lg bg-rose-500/10 border border-rose-500/20 text-rose-400 flex items-start space-x-3 mb-6">
          <AlertCircle size={16} className="shrink-0 mt-0.5" />
          <div className="text-xs">
            <div className="font-bold">Research Error</div>
            <div className="mt-0.5">{error}</div>
          </div>
        </div>
      )}

      {/* Result Presentation */}
      {report && (
        <div className="space-y-6">
          {/* Executive Summary Card */}
          <div className="p-4 rounded-lg bg-bg-surface border border-gray-800">
            <div className="flex items-center justify-between mb-2">
              <span className="text-xs font-semibold uppercase tracking-wider text-indigo-400 flex items-center space-x-1.5">
                <FileText size={14} />
                <span>Executive Summary</span>
              </span>
              <button
                onClick={handleCopy}
                className="px-2.5 py-1 text-[11px] rounded bg-gray-800 hover:bg-gray-700 text-gray-300 border border-gray-700 flex items-center space-x-1.5 transition-colors"
              >
                {copied ? <Check size={12} className="text-emerald-400" /> : <Copy size={12} />}
                <span>{copied ? 'Copied' : 'Copy Markdown'}</span>
              </button>
            </div>
            <p className="text-sm text-gray-200 leading-relaxed font-sans">{report.executive_summary}</p>
          </div>

          {/* Key Findings */}
          <div className="p-4 rounded-lg bg-bg-surface border border-gray-800">
            <div className="text-xs font-semibold uppercase tracking-wider text-gray-400 mb-3">
              Key Synthesized Findings
            </div>
            <div className="space-y-2">
              {report.key_findings.map((f, idx) => (
                <div key={idx} className="flex items-start space-x-2 text-xs text-gray-300">
                  <ArrowRight size={13} className="text-indigo-400 shrink-0 mt-0.5" />
                  <span>{f}</span>
                </div>
              ))}
            </div>
          </div>

          {/* Citations & Provenance */}
          <div className="p-4 rounded-lg bg-bg-surface border border-gray-800">
            <div className="text-xs font-semibold uppercase tracking-wider text-gray-400 mb-3 flex items-center space-x-2">
              <BookOpen size={14} />
              <span>Grounded Substrate Citations ({report.citations.length})</span>
            </div>
            <div className="space-y-3">
              {report.citations.map((c) => (
                <div
                  key={c.index}
                  className="p-3 rounded bg-black/30 border border-gray-800 flex flex-col space-y-1"
                >
                  <div className="flex items-center justify-between text-xs">
                    <span className="font-bold text-indigo-300">
                      [^{c.index}] {c.title}
                    </span>
                    <span className="text-[11px] text-gray-500 font-mono">{c.url_or_path}</span>
                  </div>
                  <blockquote className="text-[11px] text-gray-400 border-l-2 border-indigo-500/40 pl-2 italic">
                    "{c.snippet}"
                  </blockquote>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {!report && !loading && (
        <div className="flex-1 flex flex-col items-center justify-center text-gray-600 py-16 border border-dashed border-gray-800 rounded-lg">
          <Compass size={36} className="mb-2 opacity-30" />
          <div className="text-xs font-semibold text-gray-400">Deep Research Engine Ready</div>
          <p className="text-[11px] text-gray-600 mt-1 max-w-sm text-center">
            Formulate an inquiry above to run autonomous planning, source indexing, multi-document synthesis, and strict footnote citations.
          </p>
        </div>
      )}
    </div>
  );
}
