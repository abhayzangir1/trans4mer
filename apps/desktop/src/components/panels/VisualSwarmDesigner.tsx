import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { 
  Users, 
  Swords, 
  Workflow, 
  Play, 
  X, 
  CheckCircle2, 
  AlertCircle,
  Network
} from 'lucide-react';
import SwarmMap from '../layout/SwarmMap';
import { useAgents } from '../../hooks/useAgents';
import { useConversationStore } from '../../store/conversationStore';

interface SwarmResult {
  pattern: string;
  final_output: string;
  transcript: [string, string][];
}

export default function VisualSwarmDesigner({ projectId }: { projectId: string | null }) {
  const { agents } = useAgents(projectId);
  const { activeConversationId } = useConversationStore();

  // Modals state
  const [isDebateModalOpen, setIsDebateModalOpen] = useState(false);
  const [isSupervisorModalOpen, setIsSupervisorModalOpen] = useState(false);
  const [isFanoutModalOpen, setIsFanoutModalOpen] = useState(false);

  // Debate parameters
  const [debateTopic, setDebateTopic] = useState('');
  const [proponentId, setProponentId] = useState('');
  const [opponentId, setOpponentId] = useState('');
  const [rounds, setRounds] = useState(2);
  const [isDebating, setIsDebating] = useState(false);

  // Supervisor parameters
  const [supervisorId, setSupervisorId] = useState('');
  const [goal, setGoal] = useState('');
  const [milestonesText, setMilestonesText] = useState('');
  const [isDispatchingSupervisor, setIsDispatchingSupervisor] = useState(false);

  // Fanout parameters
  const [fanoutCoordinatorId, setFanoutCoordinatorId] = useState('');
  const [fanoutTasksText, setFanoutTasksText] = useState('');
  const [isDispatchingFanout, setIsDispatchingFanout] = useState(false);

  const [activeTranscript, setActiveTranscript] = useState<SwarmResult | null>(null);
  const [feedback, setFeedback] = useState<{ type: 'success' | 'error'; message: string } | null>(null);

  const handleStartDebate = async () => {
    if (!projectId || !debateTopic.trim()) return;
    const propId = proponentId || agents[0]?.id;
    const oppId = opponentId || agents[1]?.id || agents[0]?.id;

    if (!propId || !oppId) {
      setFeedback({ type: 'error', message: 'At least one agent must be present to debate.' });
      return;
    }

    setIsDebating(true);
    setFeedback(null);
    try {
      const convoId = activeConversationId || 'conv_general';
      const result = await invoke<SwarmResult>('start_swarm_debate', {
        projectId,
        conversationId: convoId,
        topic: debateTopic.trim(),
        proponentId: propId,
        opponentId: oppId,
        rounds: Number(rounds) || 2,
      });

      setActiveTranscript(result);
      setFeedback({
        type: 'success',
        message: `Debate completed! View full transcript below.`,
      });
      setIsDebateModalOpen(false);
      setDebateTopic('');
    } catch (err: any) {
      setFeedback({
        type: 'error',
        message: typeof err === 'string' ? err : err.message || 'Failed to execute debate.',
      });
    } finally {
      setIsDebating(false);
    }
  };

  const handleStartSupervisor = async () => {
    if (!projectId || !goal.trim()) return;
    const supId = supervisorId || agents[0]?.id;
    if (!supId) {
      setFeedback({ type: 'error', message: 'No agent available to serve as Supervisor.' });
      return;
    }

    const workerIds = agents.filter((a) => a.id !== supId).map((a) => a.id);
    const effectiveWorkers = workerIds.length > 0 ? workerIds : [supId];

    setIsDispatchingSupervisor(true);
    setFeedback(null);
    try {
      const convoId = activeConversationId || 'conv_general';
      const fullGoal = milestonesText.trim() ? `${goal.trim()}\n\nMilestones:\n${milestonesText.trim()}` : goal.trim();
      const result = await invoke<SwarmResult>('start_supervisor_task', {
        projectId,
        conversationId: convoId,
        goal: fullGoal,
        supervisorId: supId,
        workerIds: effectiveWorkers,
      });

      setActiveTranscript(result);
      setFeedback({
        type: 'success',
        message: `Supervisor orchestration finished! View full transcript below.`,
      });
      setIsSupervisorModalOpen(false);
      setGoal('');
    } catch (err: any) {
      setFeedback({
        type: 'error',
        message: typeof err === 'string' ? err : err.message || 'Failed to dispatch supervisor.',
      });
    } finally {
      setIsDispatchingSupervisor(false);
    }
  };

  const handleStartFanout = async () => {
    if (!projectId || !fanoutTasksText.trim()) return;
    const coordId = fanoutCoordinatorId || agents[0]?.id;
    if (!coordId) {
      setFeedback({ type: 'error', message: 'No agent available to coordinate Fan-out.' });
      return;
    }

    const subtasks = fanoutTasksText
      .split('\n')
      .map((t) => t.trim())
      .filter((t) => t.length > 0);

    const workerIds = agents.filter((a) => a.id !== coordId).map((a) => a.id);
    const effectiveWorkers = workerIds.length > 0 ? workerIds : [coordId];

    setIsDispatchingFanout(true);
    setFeedback(null);
    try {
      const convoId = activeConversationId || 'conv_general';
      const result = await invoke<SwarmResult>('start_swarm_fanout', {
        projectId,
        conversationId: convoId,
        subtasks,
        coordinatorId: coordId,
        workerIds: effectiveWorkers,
      });

      setActiveTranscript(result);
      setFeedback({
        type: 'success',
        message: `Fan-out swarm finished! View full transcript below.`,
      });
      setIsFanoutModalOpen(false);
      setFanoutTasksText('');
    } catch (err: any) {
      setFeedback({
        type: 'error',
        message: typeof err === 'string' ? err : err.message || 'Failed to dispatch fan-out.',
      });
    } finally {
      setIsDispatchingFanout(false);
    }
  };

  return (
    <div className="h-full flex flex-col bg-bg-base relative font-mono select-none">
      {/* Top Controls Toolbar */}
      <div className="h-12 border-b border-gray-800 bg-bg-surface px-4 flex items-center justify-between shrink-0">
        <div className="flex items-center space-x-2">
          <div className="p-1 rounded bg-brand-primary/10 text-brand-primary">
            <Workflow size={16} />
          </div>
          <span className="font-bold text-xs text-gray-200 uppercase tracking-wider">
            Visual Swarm Designer & Orchestrator
          </span>
        </div>

        <div className="flex items-center space-x-2">
          <button
            onClick={() => setIsDebateModalOpen(true)}
            className="px-3 py-1 text-xs rounded bg-purple-600/20 hover:bg-purple-600/30 text-purple-300 border border-purple-500/30 flex items-center space-x-1.5 transition-colors"
          >
            <Swords size={13} />
            <span>Launch Multi-Agent Debate</span>
          </button>
          <button
            onClick={() => setIsSupervisorModalOpen(true)}
            className="px-3 py-1 text-xs rounded bg-brand-primary/20 hover:bg-brand-primary/30 text-brand-primary border border-brand-primary/30 flex items-center space-x-1.5 transition-colors"
          >
            <Users size={13} />
            <span>Supervisor Orchestration</span>
          </button>
          <button
            onClick={() => setIsFanoutModalOpen(true)}
            className="px-3 py-1 text-xs rounded bg-emerald-600/20 hover:bg-emerald-600/30 text-emerald-300 border border-emerald-500/30 flex items-center space-x-1.5 transition-colors"
          >
            <Network size={13} />
            <span>Fan-Out Swarm</span>
          </button>
        </div>
      </div>

      {/* Feedback Toast */}
      {feedback && (
        <div
          className={`px-4 py-2 text-xs flex items-center justify-between border-b ${
            feedback.type === 'success'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-300'
              : 'bg-rose-500/10 border-rose-500/30 text-rose-300'
          }`}
        >
          <div className="flex items-center space-x-2">
            {feedback.type === 'success' ? <CheckCircle2 size={14} /> : <AlertCircle size={14} />}
            <span>{feedback.message}</span>
          </div>
          <button onClick={() => setFeedback(null)} className="text-gray-400 hover:text-gray-200">
            <X size={13} />
          </button>
        </div>
      )}

      {/* Main Graph Area */}
      <div className="flex-1 relative">
        <SwarmMap projectId={projectId} />
      </div>

      {/* Multi-Agent Debate Modal */}
      {isDebateModalOpen && (
        <div className="fixed inset-0 bg-black/70 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-bg-surface border border-gray-800 rounded-lg max-w-lg w-full p-6 shadow-2xl">
            <div className="flex items-center justify-between pb-3 border-b border-gray-800">
              <div className="flex items-center space-x-2 text-purple-400 font-bold text-sm">
                <Swords size={16} />
                <span>Multi-Agent Swarm Debate</span>
              </div>
              <button
                onClick={() => setIsDebateModalOpen(false)}
                className="text-gray-400 hover:text-gray-200"
              >
                <X size={16} />
              </button>
            </div>

            <div className="space-y-4 my-4 text-xs">
              <div>
                <label className="block text-gray-400 font-semibold mb-1">Debate Topic</label>
                <input
                  type="text"
                  placeholder="e.g. SQLite In-Memory vs WAL Substrate for High-Throughput Agent Logging"
                  value={debateTopic}
                  onChange={(e) => setDebateTopic(e.target.value)}
                  className="w-full bg-black/40 border border-gray-700 rounded px-3 py-2 text-gray-100 placeholder-gray-600 focus:outline-none focus:border-purple-500"
                />
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-gray-400 font-semibold mb-1">Proponent Agent</label>
                  <select
                    value={proponentId}
                    onChange={(e) => setProponentId(e.target.value)}
                    className="w-full bg-black/40 border border-gray-700 rounded px-2 py-1.5 text-gray-100 focus:outline-none focus:border-purple-500"
                  >
                    {agents.length === 0 ? (
                      <option value="" disabled>No agents found</option>
                    ) : (
                      agents.map((a) => (
                        <option key={a.id} value={a.id}>
                          {a.name || a.definitionId} ({a.role})
                        </option>
                      ))
                    )}
                  </select>
                </div>

                <div>
                  <label className="block text-gray-400 font-semibold mb-1">Critic / Opponent Agent</label>
                  <select
                    value={opponentId}
                    onChange={(e) => setOpponentId(e.target.value)}
                    className="w-full bg-black/40 border border-gray-700 rounded px-2 py-1.5 text-gray-100 focus:outline-none focus:border-purple-500"
                  >
                    {agents.length === 0 ? (
                      <option value="" disabled>No agents found</option>
                    ) : (
                      agents.map((a) => (
                        <option key={a.id} value={a.id}>
                          {a.name || a.definitionId} ({a.role})
                        </option>
                      ))
                    )}
                  </select>
                </div>
              </div>

              <div>
                <label className="block text-gray-400 font-semibold mb-1">Debate Rounds ({rounds})</label>
                <input
                  type="range"
                  min={1}
                  max={5}
                  value={rounds}
                  onChange={(e) => setRounds(Number(e.target.value))}
                  className="w-full accent-purple-500"
                />
              </div>
            </div>

            <div className="flex items-center justify-end space-x-2 pt-3 border-t border-gray-800">
              <button
                onClick={() => setIsDebateModalOpen(false)}
                className="px-3 py-1.5 text-xs text-gray-400 hover:text-gray-200"
              >
                Cancel
              </button>
              <button
                onClick={handleStartDebate}
                disabled={isDebating || !debateTopic.trim() || agents.length < 2}
                className="px-4 py-1.5 text-xs bg-purple-600 hover:bg-purple-500 text-white rounded font-medium flex items-center space-x-1.5 transition-colors disabled:opacity-50"
              >
                <Play size={12} className={isDebating ? 'animate-spin' : ''} />
                <span>{isDebating ? 'Conducting Debate...' : 'Execute Debate'}</span>
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Supervisor Orchestration Modal */}
      {isSupervisorModalOpen && (
        <div className="fixed inset-0 bg-black/70 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-bg-surface border border-gray-800 rounded-lg max-w-lg w-full p-6 shadow-2xl">
            <div className="flex items-center justify-between pb-3 border-b border-gray-800">
              <div className="flex items-center space-x-2 text-brand-primary font-bold text-sm">
                <Users size={16} />
                <span>Supervisor Task Orchestration</span>
              </div>
              <button
                onClick={() => setIsSupervisorModalOpen(false)}
                className="text-gray-400 hover:text-gray-200"
              >
                <X size={16} />
              </button>
            </div>

            <div className="space-y-4 my-4 text-xs">
              <div>
                <label className="block text-gray-400 font-semibold mb-1">Supervisor Agent</label>
                <select
                  value={supervisorId}
                  onChange={(e) => setSupervisorId(e.target.value)}
                  className="w-full bg-black/40 border border-gray-700 rounded px-2 py-1.5 text-gray-100 focus:outline-none focus:border-brand-primary"
                >
                  {agents.length === 0 ? (
                    <option value="" disabled>No agents found in workspace</option>
                  ) : (
                    agents.map((a) => (
                      <option key={a.id} value={a.id}>
                        {a.name || a.definitionId} ({a.role})
                      </option>
                    ))
                  )}
                </select>
              </div>

              <div>
                <label className="block text-gray-400 font-semibold mb-1">Overall Macro Goal</label>
                <input
                  type="text"
                  placeholder="e.g. Implement full end-to-end integration of SQLite vector search"
                  value={goal}
                  onChange={(e) => setGoal(e.target.value)}
                  className="w-full bg-black/40 border border-gray-700 rounded px-3 py-2 text-gray-100 placeholder-gray-600 focus:outline-none focus:border-brand-primary"
                />
              </div>

              <div>
                <label className="block text-gray-400 font-semibold mb-1">
                  Decomposed Milestones / Guidance (One per line)
                </label>
                <textarea
                  rows={4}
                  value={milestonesText}
                  onChange={(e) => setMilestonesText(e.target.value)}
                  placeholder="1. Design domain architecture&#10;2. Implement core engine logic&#10;3. Verify test coverage"
                  className="w-full bg-black/40 border border-gray-700 rounded p-2 text-gray-100 font-mono text-[11px] focus:outline-none focus:border-brand-primary"
                />
              </div>
            </div>

            <div className="flex items-center justify-end space-x-2 pt-3 border-t border-gray-800">
              <button
                onClick={() => setIsSupervisorModalOpen(false)}
                className="px-3 py-1.5 text-xs text-gray-400 hover:text-gray-200"
              >
                Cancel
              </button>
              <button
                onClick={handleStartSupervisor}
                disabled={isDispatchingSupervisor || !goal.trim() || agents.length === 0}
                className="px-4 py-1.5 text-xs bg-brand-primary hover:bg-brand-primary/90 text-white rounded font-medium flex items-center space-x-1.5 transition-colors disabled:opacity-50"
              >
                <Play size={12} className={isDispatchingSupervisor ? 'animate-spin' : ''} />
                <span>{isDispatchingSupervisor ? 'Dispatching...' : 'Dispatch Supervisor'}</span>
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Fan-Out Swarm Modal */}
      {isFanoutModalOpen && (
        <div className="fixed inset-0 bg-black/70 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-bg-surface border border-gray-800 rounded-lg max-w-lg w-full p-6 shadow-2xl">
            <div className="flex items-center justify-between pb-3 border-b border-gray-800">
              <div className="flex items-center space-x-2 text-emerald-400 font-bold text-sm">
                <Network size={16} />
                <span>Parallel Fan-Out Swarm</span>
              </div>
              <button
                onClick={() => setIsFanoutModalOpen(false)}
                className="text-gray-400 hover:text-gray-200"
              >
                <X size={16} />
              </button>
            </div>

            <div className="space-y-4 my-4 text-xs">
              <div>
                <label className="block text-gray-400 font-semibold mb-1">Coordinator Agent</label>
                <select
                  value={fanoutCoordinatorId}
                  onChange={(e) => setFanoutCoordinatorId(e.target.value)}
                  className="w-full bg-black/40 border border-gray-700 rounded px-2 py-1.5 text-gray-100 focus:outline-none focus:border-emerald-500"
                >
                  {agents.length === 0 ? (
                    <option value="" disabled>No agents found in workspace</option>
                  ) : (
                    agents.map((a) => (
                      <option key={a.id} value={a.id}>
                        {a.name || a.definitionId} ({a.role})
                      </option>
                    ))
                  )}
                </select>
              </div>

              <div>
                <label className="block text-gray-400 font-semibold mb-1">
                  Parallel Subtasks (One per worker, one per line)
                </label>
                <textarea
                  rows={4}
                  value={fanoutTasksText}
                  onChange={(e) => setFanoutTasksText(e.target.value)}
                  placeholder="Task 1: Benchmark latency&#10;Task 2: Audit cryptographic tokens"
                  className="w-full bg-black/40 border border-gray-700 rounded p-2 text-gray-100 font-mono text-[11px] focus:outline-none focus:border-emerald-500"
                />
              </div>
            </div>

            <div className="flex items-center justify-end space-x-2 pt-3 border-t border-gray-800">
              <button
                onClick={() => setIsFanoutModalOpen(false)}
                className="px-3 py-1.5 text-xs text-gray-400 hover:text-gray-200"
              >
                Cancel
              </button>
              <button
                onClick={handleStartFanout}
                disabled={isDispatchingFanout || !fanoutTasksText.trim() || agents.length === 0}
                className="px-4 py-1.5 text-xs bg-emerald-600 hover:bg-emerald-500 text-white rounded font-medium flex items-center space-x-1.5 transition-colors disabled:opacity-50"
              >
                <Play size={12} className={isDispatchingFanout ? 'animate-spin' : ''} />
                <span>{isDispatchingFanout ? 'Executing Fan-Out...' : 'Execute Fan-Out'}</span>
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Full Transcript Viewer Modal */}
      {activeTranscript && (
        <div className="fixed inset-0 bg-black/80 backdrop-blur-sm z-50 flex items-center justify-center p-6">
          <div className="bg-bg-surface border border-gray-700 rounded-xl max-w-3xl w-full max-h-[85vh] flex flex-col shadow-2xl overflow-hidden">
            <div className="h-12 px-6 border-b border-gray-800 flex items-center justify-between">
              <span className="font-bold text-gray-200">Swarm Execution Transcript: {activeTranscript.pattern}</span>
              <button onClick={() => setActiveTranscript(null)} className="text-gray-400 hover:text-white">
                <X size={18} />
              </button>
            </div>
            <div className="flex-1 overflow-y-auto p-6 space-y-4 font-mono text-xs select-text">
              <div className="p-3 bg-brand-primary/10 border border-brand-primary/30 rounded-lg text-brand-primary">
                <div className="font-bold mb-1">Final Synthesis:</div>
                <div className="text-gray-200 whitespace-pre-wrap leading-relaxed">{activeTranscript.final_output}</div>
              </div>
              <div className="space-y-3">
                <div className="font-bold text-gray-400 uppercase tracking-wider text-[10px]">Turn-by-turn Transcript:</div>
                {activeTranscript.transcript?.map(([speaker, utterance], i) => (
                  <div key={i} className="p-3 rounded-lg bg-black/40 border border-gray-800">
                    <div className="text-amber-400 font-semibold mb-1">@{speaker}:</div>
                    <div className="text-gray-300 whitespace-pre-wrap leading-relaxed">{utterance}</div>
                  </div>
                ))}
              </div>
            </div>
            <div className="p-4 border-t border-gray-800 flex justify-end">
              <button
                onClick={() => setActiveTranscript(null)}
                className="px-4 py-2 bg-gray-800 hover:bg-gray-700 text-gray-200 rounded-lg text-xs font-semibold"
              >
                Close
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
