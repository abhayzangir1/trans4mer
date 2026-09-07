import { useState, useEffect } from 'react';
import { 
  Settings, 
  Cpu, 
  Layers, 
  ShieldCheck, 
  DollarSign, 
  Server, 
  X, 
  Check, 
  HardDrive,
  Clock,
  MessageSquare,
  RefreshCw,
  Trash2,
  Play,
  Activity,
  GitPullRequest
} from 'lucide-react';
import { useSettings, CompactionConfig, DiffReviewConfig, PerformanceProfile } from '../../hooks/useSettings';
import { useProjectStore } from '../../store/projectStore';
import { useUiStore } from '../../store/uiStore';
import { invoke } from '@tauri-apps/api/core';
import { McpInspectorModal } from './McpInspectorModal';

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

type TabType = 'general' | 'performance' | 'compaction' | 'diff' | 'cost' | 'mcp' | 'automations' | 'telegram' | 'langfuse' | 'github';

export default function SettingsModal({ isOpen, onClose }: SettingsModalProps) {
  const { settings, saveSettings } = useSettings();
  const activeProjectId = useProjectStore(state => state.activeProjectId);
  const [activeTab, setActiveTab] = useState<TabType>('general');

  // General & BYOK Keys
  const [theme, setTheme] = useState(settings.theme);
  const [provider, setProvider] = useState(settings.llmProvider);
  const [openaiKey, setOpenaiKey] = useState(settings.openaiApiKey);
  const [anthropicKey, setAnthropicKey] = useState(settings.anthropicApiKey);
  const [geminiKey, setGeminiKey] = useState('');
  const [openrouterKey, setOpenrouterKey] = useState('');
  const [ollamaEndpoint, setOllamaEndpoint] = useState('http://localhost:11434');

  // Dynamic Model Scanner
  const [scanProvider, setScanProvider] = useState('ollama');
  const [scannedModels, setScannedModels] = useState<string[]>([]);
  const [isScanning, setIsScanning] = useState(false);
  const [scanError, setScanError] = useState('');
  const [selectedScannedModel, setSelectedScannedModel] = useState('qwen2.5-coder:7b');
  const [modelGuidance, setModelGuidance] = useState<any | null>(null);

  useEffect(() => {
    if (selectedScannedModel) {
      invoke('get_model_guidance', { model: selectedScannedModel })
        .then((guidance: any) => setModelGuidance(guidance))
        .catch(() => setModelGuidance(null));
    } else {
      setModelGuidance(null);
    }
  }, [selectedScannedModel]);

  // Performance & Compaction
  const [perfProfile, setPerfProfile] = useState<PerformanceProfile>(settings.performanceProfile);
  const [compaction, setCompaction] = useState<CompactionConfig>(settings.compaction);
  const [diffReview, setDiffReview] = useState<DiffReviewConfig>(settings.diffReview);

  // Cost guard state
  const [costData, setCostData] = useState<{ daily_usd: number; monthly_usd: number; budget: any } | null>(null);
  const [monthlyCeiling, setMonthlyCeiling] = useState<string>('50');
  const [dailyCeiling, setDailyCeiling] = useState<string>('5');
  const [hardBlock, setHardBlock] = useState<boolean>(true);

  // MCP Servers state
  const [mcpServers, setMcpServers] = useState<any[]>([]);
  const [newMcpName, setNewMcpName] = useState('');
  const [newMcpCommand, setNewMcpCommand] = useState('');
  const [newMcpToken, setNewMcpToken] = useState('');
  const [mcpAuthStatus, setMcpAuthStatus] = useState<Record<string, boolean>>({});
  const [inspectingServer, setInspectingServer] = useState<{ name: string; command: string } | null>(null);

  // Automations & Nightly Dreaming
  const [nightlyDreamingEnabled, setNightlyDreamingEnabled] = useState(false);
  const [dreamingModel, setDreamingModel] = useState('qwen2.5-coder:3b');
  const [scheduledTasks, setScheduledTasks] = useState<any[]>([]);
  const [isDreaming, setIsDreaming] = useState(false);
  const [dreamResult, setDreamResult] = useState('');
  const [newScheduleCron, setNewScheduleCron] = useState('@every 2h');
  const [newScheduleSummary, setNewScheduleSummary] = useState('');
  const [newSchedulePrompt, setNewSchedulePrompt] = useState('');

  // Telegram Gateway
  const [telegramEnabled, setTelegramEnabled] = useState(false);
  const [telegramBotToken, setTelegramBotToken] = useState('');
  const [telegramAllowedChatIds, setTelegramAllowedChatIds] = useState('');
  const [telegramTestStatus, setTelegramTestStatus] = useState('');
  const [isTestingTelegram, setIsTestingTelegram] = useState(false);

  // Langfuse Observability
  const [langfuseHost, setLangfuseHost] = useState('http://localhost:3000');
  const [langfuseEnabled, setLangfuseEnabled] = useState(false);
  const [langfuseCapturePrompts, setLangfuseCapturePrompts] = useState(false);
  const [langfusePublicKey, setLangfusePublicKey] = useState('');
  const [langfuseSecretKey, setLangfuseSecretKey] = useState('');
  const [langfuseHasPublicKey, setLangfuseHasPublicKey] = useState(false);
  const [langfuseHasSecretKey, setLangfuseHasSecretKey] = useState(false);
  const [langfuseTestStatus, setLangfuseTestStatus] = useState('');
  const [isTestingLangfuse, setIsTestingLangfuse] = useState(false);

  // GitHub Integration
  const [githubPat, setGithubPat] = useState('');
  const [githubConfigured, setGithubConfigured] = useState(false);
  const [githubMasked, setGithubMasked] = useState<string | null>(null);
  const [githubImportUrl, setGithubImportUrl] = useState('');
  const [githubImporting, setGithubImporting] = useState(false);
  const [githubImportMsg, setGithubImportMsg] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  const [saving, setSaving] = useState(false);
  const [savedSuccess, setSavedSuccess] = useState(false);

  useEffect(() => {
    setTheme(settings.theme);
    setProvider(settings.llmProvider);
    setOpenaiKey(settings.openaiApiKey);
    setAnthropicKey(settings.anthropicApiKey);
    setGeminiKey(settings.geminiApiKey || '');
    setOpenrouterKey(settings.openrouterApiKey || '');
    setOllamaEndpoint(settings.ollamaEndpoint || 'http://127.0.0.1:11434');
    setNightlyDreamingEnabled(settings.features?.nightly_dreaming_enabled ?? false);
    if (settings.features?.dreaming_model) {
      setDreamingModel(settings.features.dreaming_model);
    }
    setPerfProfile(settings.performanceProfile);
    setCompaction(settings.compaction);
    setDiffReview(settings.diffReview);
  }, [settings, isOpen]);

  useEffect(() => {
    if (!isOpen) return;

    if (activeTab === 'cost') {
      invoke<any>('get_cost_summary', { provider: 'openai', projectId: null })
        .then(res => {
          setCostData(res);
          if (res?.budget) {
            setMonthlyCeiling(res.budget.monthly_ceiling_usd?.toString() || '50');
            setDailyCeiling(res.budget.daily_ceiling_usd?.toString() || '5');
            setHardBlock(Boolean(res.budget.hard_block));
          }
        })
        .catch(console.error);
    } else if (activeTab === 'mcp') {
      loadMcpServers();
    } else if (activeTab === 'automations') {
      if (activeProjectId) {
        invoke<any[]>('list_scheduled_tasks', { projectId: activeProjectId })
          .then(res => setScheduledTasks(res || []))
          .catch(console.error);
      }
    } else if (activeTab === 'telegram') {
      invoke<any>('get_gateway_status')
        .then(res => {
          if (res) {
            setTelegramEnabled(res.telegram_enabled);
          }
        })
        .catch(console.error);
    } else if (activeTab === 'langfuse') {
      loadLangfuseConfig();
    } else if (activeTab === 'github') {
      loadGithubStatus();
    }
  }, [isOpen, activeTab, activeProjectId]);

  if (!isOpen) return null;

  // BYOK Model Scanner trigger
  const handleScanModels = async () => {
    setIsScanning(true);
    setScanError('');
    setScannedModels([]);

    let key: string | null = null;
    let endpoint: string | null = null;

    if (scanProvider === 'openai') key = openaiKey;
    if (scanProvider === 'anthropic') key = anthropicKey;
    if (scanProvider === 'google') key = geminiKey;
    if (scanProvider === 'openrouter') key = openrouterKey;
    if (scanProvider === 'ollama') endpoint = ollamaEndpoint;

    try {
      const models = await invoke<string[]>('list_available_models', {
        name: scanProvider,
        apiKey: key && key.trim() ? key.trim() : null,
        endpoint: endpoint && endpoint.trim() ? endpoint.trim() : null,
      });
      setScannedModels(models);
      if (models.length > 0) setSelectedScannedModel(models[0]);
    } catch (err: any) {
      setScanError(typeof err === 'string' ? err : err?.message || 'Scan failed');
    } finally {
      setIsScanning(false);
    }
  };

  const handleTriggerDreamingNow = async () => {
    if (!activeProjectId || isDreaming) return;
    setIsDreaming(true);
    setDreamResult('Dreaming in progress... Analyzing last 24 hours of logs...');
    try {
      const summary = await invoke<any>('trigger_nightly_dreaming', { projectId: activeProjectId });
      setDreamResult(`✓ Consolidation complete! Analyzed ${summary.conversations_analyzed} conversation turns.`);
    } catch (err: any) {
      setDreamResult(`Error during consolidation: ${err}`);
    } finally {
      setIsDreaming(false);
    }
  };

  const handleCreateSchedule = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeProjectId || !newScheduleCron.trim() || !newSchedulePrompt.trim()) return;

    try {
      const created = await invoke<any>('create_scheduled_task', {
        projectId: activeProjectId,
        conversationId: 'scheduled-automations',
        targetAgentId: 'boss',
        cronExpression: newScheduleCron.trim(),
        humanReadable: newScheduleSummary.trim() || newSchedulePrompt.slice(0, 30),
        actionPrompt: newSchedulePrompt.trim(),
      });
      setScheduledTasks(prev => [created, ...prev]);
      setNewScheduleSummary('');
      setNewSchedulePrompt('');
      useUiStore.getState().showToast('success', 'Scheduled task created');
    } catch (err: any) {
      console.error('Failed to create scheduled task:', err);
      useUiStore.getState().showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to create scheduled task');
    }
  };

  const handleDeleteSchedule = async (taskId: string) => {
    if (!activeProjectId) return;
    try {
      await invoke('delete_scheduled_task', { projectId: activeProjectId, taskId });
      setScheduledTasks(prev => prev.filter(t => t.id !== taskId));
      useUiStore.getState().showToast('info', 'Scheduled task deleted');
    } catch (err: any) {
      console.error('Failed to delete scheduled task:', err);
      useUiStore.getState().showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to delete scheduled task');
    }
  };

  const handleToggleSchedule = async (taskId: string, currentActive: boolean) => {
    if (!activeProjectId) return;
    try {
      await invoke('toggle_scheduled_task', { projectId: activeProjectId, taskId, isActive: !currentActive });
      setScheduledTasks(prev => prev.map(t => t.id === taskId ? { ...t, is_active: !currentActive } : t));
      useUiStore.getState().showToast('info', currentActive ? 'Automation disabled' : 'Automation enabled');
    } catch (err: any) {
      console.error('Failed to toggle scheduled task:', err);
      useUiStore.getState().showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to toggle scheduled task');
    }
  };

  const loadMcpServers = async () => {
    try {
      const list = await invoke<any[]>('list_mcp_servers');
      setMcpServers(list || []);
      if (list && list.length > 0) {
        const statuses: Record<string, boolean> = {};
        for (const s of list) {
          try {
            statuses[s.name] = await invoke<boolean>('get_mcp_auth_status', { name: s.name });
          } catch {
            statuses[s.name] = false;
          }
        }
        setMcpAuthStatus(statuses);
      } else {
        setMcpAuthStatus({});
      }
    } catch (e) {
      console.error('Failed to load MCP servers:', e);
    }
  };

  const handleAddMcp = async () => {
    if (!newMcpName.trim() || !newMcpCommand.trim()) return;
    try {
      await invoke('register_mcp_server', {
        name: newMcpName.trim(),
        command: newMcpCommand.trim(),
        args: [],
        env: {},
        autoLaunch: true,
        notes: null,
      });
      if (newMcpToken.trim()) {
        await invoke('set_mcp_auth_token', {
          name: newMcpName.trim(),
          token: newMcpToken.trim(),
        });
      }
      setNewMcpName('');
      setNewMcpCommand('');
      setNewMcpToken('');
      await loadMcpServers();
      useUiStore.getState().showToast('success', `MCP server ${newMcpName.trim()} registered`);
    } catch (err: any) {
      console.error('Failed to register MCP server:', err);
      useUiStore.getState().showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to register MCP server');
    }
  };

  const handleToggleMcpApproval = async (name: string, approved: boolean) => {
    try {
      await invoke('toggle_mcp_approval', { name, approved: !approved });
      await loadMcpServers();
      useUiStore.getState().showToast('info', `MCP approval for ${name} updated`);
    } catch (err: any) {
      console.error('Failed to toggle MCP approval:', err);
      useUiStore.getState().showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to toggle MCP approval');
    }
  };

  const handleDeleteMcp = async (name: string) => {
    try {
      await invoke('delete_mcp_server', { name });
      await loadMcpServers();
      useUiStore.getState().showToast('info', `MCP server ${name} removed`);
    } catch (err: any) {
      console.error('Failed to delete MCP server:', err);
      useUiStore.getState().showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to delete MCP server');
    }
  };

  const handleTestTelegram = async () => {
    if (!telegramBotToken.trim()) return;
    setIsTestingTelegram(true);
    setTelegramTestStatus('Connecting to Telegram Bot API...');
    try {
      const res = await invoke<string>('test_telegram_connection', { botToken: telegramBotToken.trim() });
      setTelegramTestStatus(`✓ ${res}`);
    } catch (err: any) {
      setTelegramTestStatus(`❌ ${err}`);
    } finally {
      setIsTestingTelegram(false);
    }
  };

  const handleSaveTelegram = async () => {
    try {
      const parsedChatIds = telegramAllowedChatIds
        .split(',')
        .map(s => s.trim())
        .filter(s => s.length > 0)
        .map(s => parseInt(s, 10))
        .filter(n => !isNaN(n));

      await invoke('update_telegram_config', {
        enabled: telegramEnabled,
        botToken: telegramBotToken.trim() ? telegramBotToken.trim() : null,
        allowedChatIds: parsedChatIds,
        defaultProjectId: activeProjectId || null,
      });
      setSavedSuccess(true);
      setTimeout(() => setSavedSuccess(false), 2500);
      useUiStore.getState().showToast('success', 'Telegram gateway configuration saved');
    } catch (err: any) {
      console.error('Failed to save Telegram config:', err);
      useUiStore.getState().showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to save Telegram configuration');
    }
  };

  const loadLangfuseConfig = async () => {
    try {
      const res = await invoke<{
        host: string;
        enabled: boolean;
        capture_prompts: boolean;
        has_public_key: boolean;
        has_secret_key: boolean;
        public_key_masked: string | null;
      }>('get_langfuse_config');
      if (res) {
        setLangfuseHost(res.host || 'http://localhost:3000');
        setLangfuseEnabled(res.enabled);
        setLangfuseCapturePrompts(res.capture_prompts);
        setLangfuseHasPublicKey(res.has_public_key);
        setLangfuseHasSecretKey(res.has_secret_key);
        if (res.has_public_key) setLangfusePublicKey('********');
        if (res.has_secret_key) setLangfuseSecretKey('********');
      }
    } catch (err) {
      console.error('Failed to load Langfuse config', err);
    }
  };

  const handleTestLangfuse = async () => {
    if (!langfuseHost.trim()) return;
    setIsTestingLangfuse(true);
    setLangfuseTestStatus('Testing connection to Langfuse instance...');
    try {
      const res = await invoke<string>('test_langfuse_connection', {
        host: langfuseHost.trim(),
        publicKey: langfusePublicKey.trim() === '********' ? null : (langfusePublicKey.trim() || null),
        secretKey: langfuseSecretKey.trim() === '********' ? null : (langfuseSecretKey.trim() || null),
      });
      setLangfuseTestStatus(`✓ ${res}`);
    } catch (err: any) {
      setLangfuseTestStatus(`❌ ${err}`);
    } finally {
      setIsTestingLangfuse(false);
    }
  };

  const loadGithubStatus = async () => {
    try {
      const res = await invoke<{ is_configured: boolean; masked_token: string | null }>('get_github_status');
      if (res) {
        setGithubConfigured(res.is_configured);
        setGithubMasked(res.masked_token);
        if (res.is_configured) setGithubPat('********');
      }
    } catch (err) {
      console.error('Failed to load GitHub status', err);
    }
  };

  const handleSaveGithubPat = async () => {
    try {
      if (githubPat.trim() !== '********') {
        await invoke('save_github_pat', { pat: githubPat.trim() });
      }
      setGithubPat('********');
      await loadGithubStatus();
      useUiStore.getState().showToast('success', 'GitHub Personal Access Token saved to OS Keyring');
    } catch (err: any) {
      console.error('Failed to save GitHub PAT', err);
      useUiStore.getState().showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to save GitHub token');
    }
  };

  const handleDeleteGithubPat = async () => {
    try {
      await invoke('delete_github_pat');
      setGithubPat('');
      await loadGithubStatus();
      useUiStore.getState().showToast('info', 'GitHub Personal Access Token removed from OS Keyring');
    } catch (err: any) {
      console.error('Failed to delete GitHub PAT', err);
      useUiStore.getState().showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to delete GitHub token');
    }
  };

  const handleImportGithubIssue = async () => {
    if (!activeProjectId) {
      useUiStore.getState().showToast('error', 'Please select or create a project first before importing an issue');
      return;
    }
    if (!githubImportUrl.trim()) return;
    setGithubImporting(true);
    setGithubImportMsg(null);
    try {
      const res = await invoke<any>('import_github_issue', {
        projectId: activeProjectId,
        issueRef: githubImportUrl.trim(),
      });
      setGithubImportMsg({
        type: 'success',
        text: `Issue #${res.issue_number} imported successfully! Delegated Boss Agent task created.`,
      });
      setGithubImportUrl('');
      useUiStore.getState().showToast('success', `GitHub Issue #${res.issue_number} imported`);
    } catch (err: any) {
      const msg = typeof err === 'string' ? err : err?.message || 'Failed to import GitHub issue';
      setGithubImportMsg({ type: 'error', text: msg });
      useUiStore.getState().showToast('error', msg);
    } finally {
      setGithubImporting(false);
    }
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      await saveSettings({
        theme,
        llmProvider: provider,
        openaiApiKey: openaiKey,
        anthropicApiKey: anthropicKey,
        geminiApiKey: geminiKey,
        openrouterApiKey: openrouterKey,
        ollamaEndpoint,
        features: {
          nightly_dreaming_enabled: nightlyDreamingEnabled,
          dreaming_model: dreamingModel || null,
          dynamic_model_scan_enabled: true,
          human_in_the_loop_sync: true,
        },
        performanceProfile: perfProfile,
        compaction,
        diffReview,
      });

      if (activeTab === 'cost') {
        let monthly: number | null = null;
        let daily: number | null = null;
        
        if (monthlyCeiling.trim() !== '') {
          monthly = parseFloat(monthlyCeiling);
          if (isNaN(monthly)) {
            useUiStore.getState().showToast('error', 'Invalid Monthly Budget. Please enter a valid number.');
            setSaving(false);
            return;
          }
        }
        
        if (dailyCeiling.trim() !== '') {
          daily = parseFloat(dailyCeiling);
          if (isNaN(daily)) {
            useUiStore.getState().showToast('error', 'Invalid Daily Budget. Please enter a valid number.');
            setSaving(false);
            return;
          }
        }

        await invoke('set_cost_budget', {
          provider: 'openai',
          projectId: null,
          monthlyCeilingUsd: monthly,
          dailyCeilingUsd: daily,
          hardBlock,
        });
      }

      if (activeTab === 'langfuse') {
        await invoke('save_langfuse_config', {
          host: langfuseHost.trim(),
          enabled: langfuseEnabled,
          capturePrompts: langfuseCapturePrompts,
          publicKey: langfusePublicKey.trim() === '********' ? null : (langfusePublicKey.trim() || null),
          secretKey: langfuseSecretKey.trim() === '********' ? null : (langfuseSecretKey.trim() || null),
        });
        await loadLangfuseConfig();
      }

      setSavedSuccess(true);
      setTimeout(() => setSavedSuccess(false), 2500);
      useUiStore.getState().showToast('success', 'Settings saved successfully');
    } catch (e: any) {
      console.error('Failed to save settings', e);
      useUiStore.getState().showToast('error', typeof e === 'string' ? e : e?.message || 'Failed to save settings');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-md p-4">
      <div className="w-[780px] max-h-[88vh] rounded-xl border border-zinc-800 bg-zinc-950 flex flex-col shadow-2xl overflow-hidden font-sans">
        {/* Modal Header */}
        <div className="px-6 py-4 border-b border-zinc-800 flex items-center justify-between bg-zinc-900/60">
          <div className="flex items-center space-x-2">
            <Settings size={20} className="text-zinc-300" />
            <span className="text-lg font-bold text-white">Trans4mers Control Center</span>
          </div>
          <button
            onClick={onClose}
            className="text-zinc-400 hover:text-white p-1 rounded-lg hover:bg-zinc-800 transition"
          >
            <X size={18} />
          </button>
        </div>

        {/* Navigation Tabs */}
        <div className="flex border-b border-zinc-800 bg-zinc-900/30 px-6 space-x-1 overflow-x-auto text-xs">
          {[
            { id: 'general', label: 'Models & BYOK', icon: Settings },
            { id: 'automations', label: 'Automations & Dreaming', icon: Clock },
            { id: 'telegram', label: 'Telegram Gateway', icon: MessageSquare },
            { id: 'performance', label: 'Hardware Profile', icon: Cpu },
            { id: 'compaction', label: 'Context Compaction', icon: Layers },
            { id: 'diff', label: 'Diff Review & Trust', icon: ShieldCheck },
            { id: 'cost', label: 'Cost Guard', icon: DollarSign },
            { id: 'mcp', label: 'MCP Registry', icon: Server },
            { id: 'langfuse', label: 'Observability', icon: Activity },
            { id: 'github', label: 'GitHub Sync', icon: GitPullRequest },
          ].map(tab => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id as TabType)}
                className={`flex items-center space-x-2 px-3 py-3 border-b-2 font-medium transition whitespace-nowrap ${
                  isActive
                    ? 'border-brand-primary text-white bg-zinc-800/40'
                    : 'border-transparent text-zinc-400 hover:text-zinc-200'
                }`}
              >
                <Icon size={14} />
                <span>{tab.label}</span>
              </button>
            );
          })}
        </div>

        {/* Tab Content Body */}
        <div className="flex-1 overflow-y-auto p-6 text-sm text-zinc-300 space-y-4">
          {/* TAB 1: General & Models */}
          {activeTab === 'general' && (
            <div className="space-y-4">
              <div>
                <label className="block text-xs font-semibold text-zinc-400 mb-1">Theme</label>
                <select
                  value={theme}
                  onChange={e => setTheme(e.target.value as 'light' | 'dark')}
                  className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-white text-xs focus:outline-none focus:border-zinc-600"
                >
                  <option value="dark">Dark Theme</option>
                  <option value="light">Light Theme</option>
                </select>
              </div>

              <div>
                <label className="block text-xs font-semibold text-zinc-400 mb-1">Default LLM Gateway Provider</label>
                <select
                  value={provider}
                  onChange={e => setProvider(e.target.value)}
                  className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-white text-xs focus:outline-none focus:border-zinc-600"
                >
                  <option value="ollama">Ollama (Sovereign Local Hardware • Zero Cost)</option>
                  <option value="openai">OpenAI (GPT-4o, o1, o3-mini)</option>
                  <option value="anthropic">Anthropic (Claude 3.5 Sonnet, Claude 3.7)</option>
                  <option value="google">Google Gemini (Gemini 2.0 Flash, 1.5 Pro)</option>
                </select>
              </div>

              {provider === 'ollama' && (
                <div className="rounded-lg bg-emerald-950/20 border border-emerald-500/30 p-3 text-xs text-emerald-300 space-y-1">
                  <div className="flex items-center space-x-2 font-bold">
                    <HardDrive size={14} />
                    <span>Sovereign Local Hardware Mode Active</span>
                  </div>
                  <p>
                    Trans4mers connects directly to your local Ollama daemon at <code className="bg-black/40 px-1 py-0.5 rounded font-mono">{ollamaEndpoint}</code>. 100% private, zero egress.
                  </p>
                </div>
              )}

              {/* Dynamic Model Scanner Box */}
              <div className="rounded-lg bg-zinc-900/60 border border-zinc-800 p-4 space-y-3">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="font-semibold text-xs text-white">Dynamic BYOK Model Scanner</div>
                    <div className="text-[11px] text-zinc-400">Queries the provider's native endpoint live to discover available models without hardcoding.</div>
                  </div>
                  <button
                    onClick={handleScanModels}
                    disabled={isScanning}
                    className="flex items-center space-x-1 px-3 py-1.5 rounded bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-white text-xs font-semibold"
                  >
                    <RefreshCw size={12} className={isScanning ? 'animate-spin' : ''} />
                    <span>{isScanning ? 'Scanning...' : 'Scan Models via API'}</span>
                  </button>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className="block text-[11px] text-zinc-400 mb-1">Target Provider to Scan</label>
                    <select
                      value={scanProvider}
                      onChange={e => setScanProvider(e.target.value)}
                      className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-white text-xs"
                    >
                      <option value="ollama">Ollama (Local /api/tags)</option>
                      <option value="openai">OpenAI (/v1/models)</option>
                      <option value="anthropic">Anthropic (/v1/models)</option>
                      <option value="google">Google Gemini (/v1beta/models)</option>
                      <option value="openrouter">OpenRouter (/api/v1/models)</option>
                    </select>
                  </div>

                  <div>
                    <label className="block text-[11px] text-zinc-400 mb-1">
                      {scannedModels.length > 0 ? `Available Models (${scannedModels.length} found)` : 'Inspect Model Guidance'}
                    </label>
                    {scannedModels.length > 0 ? (
                      <select
                        value={selectedScannedModel}
                        onChange={e => setSelectedScannedModel(e.target.value)}
                        className="w-full rounded bg-zinc-900 border border-emerald-500/50 p-2 text-emerald-300 font-mono text-xs"
                      >
                        {scannedModels.map(m => (
                          <option key={m} value={m}>{m}</option>
                        ))}
                      </select>
                    ) : (
                      <div className="flex space-x-2">
                        <input
                          type="text"
                          value={selectedScannedModel}
                          onChange={e => setSelectedScannedModel(e.target.value)}
                          placeholder="e.g. qwen2.5-coder:7b, claude-3-5-sonnet"
                          className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-white font-mono text-xs focus:outline-none focus:border-zinc-600"
                        />
                      </div>
                    )}
                    {scanError && <div className="text-red-400 text-xs mt-1">{scanError}</div>}
                  </div>
                </div>

                {/* Sovereign Offline Model Guidance Card */}
                {modelGuidance && (
                  <div className="mt-3 rounded-md bg-zinc-950/80 border border-zinc-700/60 p-3 space-y-2 text-xs">
                    <div className="flex items-center justify-between border-b border-zinc-800 pb-2">
                      <div className="flex items-center space-x-2">
                        <Cpu size={14} className="text-purple-400" />
                        <span className="font-semibold text-zinc-100">{modelGuidance.display_name}</span>
                        <span className="text-[10px] font-mono text-zinc-400 bg-zinc-800 px-1.5 py-0.5 rounded">
                          {modelGuidance.model_id}
                        </span>
                      </div>
                      <div className="flex items-center space-x-1.5">
                        <span className={`text-[10px] px-2 py-0.5 rounded-full font-medium ${
                          modelGuidance.family === 'AgenticCoding' ? 'bg-purple-900/60 text-purple-200 border border-purple-700/50' :
                          modelGuidance.family === 'GeneralReasoning' ? 'bg-blue-900/60 text-blue-200 border border-blue-700/50' :
                          modelGuidance.family === 'FastChat' ? 'bg-emerald-900/60 text-emerald-200 border border-emerald-700/50' :
                          modelGuidance.family === 'Embedding' ? 'bg-amber-900/60 text-amber-200 border border-amber-700/50' :
                          'bg-zinc-800 text-zinc-300'
                        }`}>
                          {modelGuidance.family === 'AgenticCoding' ? 'Agentic Coding' :
                           modelGuidance.family === 'GeneralReasoning' ? 'General Reasoning' :
                           modelGuidance.family === 'FastChat' ? 'Fast Chat' :
                           modelGuidance.family === 'Embedding' ? 'Embedding' :
                           modelGuidance.family === 'VisionMultimodal' ? 'Vision / Multimodal' :
                           modelGuidance.family}
                        </span>
                        <span className="text-[10px] px-2 py-0.5 rounded-full bg-zinc-800 text-zinc-300 border border-zinc-700/50">
                          {modelGuidance.size_class}
                        </span>
                      </div>
                    </div>

                    <p className="text-[11px] text-zinc-300 leading-relaxed">
                      {modelGuidance.description}
                    </p>

                    <div className="grid grid-cols-3 gap-2 pt-1 text-[10px]">
                      <div className="flex items-center space-x-1 bg-zinc-900/90 p-1.5 rounded border border-zinc-800/80">
                        <span className={modelGuidance.supports_tools ? "text-emerald-400 font-bold" : "text-zinc-500"}>
                          {modelGuidance.supports_tools ? "✓" : "✗"}
                        </span>
                        <span className="text-zinc-300">
                          {modelGuidance.supports_tools ? "Native Tools" : "No Native Tools"}
                        </span>
                      </div>
                      <div className="flex items-center space-x-1 bg-zinc-900/90 p-1.5 rounded border border-zinc-800/80">
                        <span className={modelGuidance.supports_streaming ? "text-emerald-400 font-bold" : "text-zinc-500"}>
                          {modelGuidance.supports_streaming ? "✓" : "✗"}
                        </span>
                        <span className="text-zinc-300">Streaming</span>
                      </div>
                      <div className="flex items-center space-x-1 bg-zinc-900/90 p-1.5 rounded border border-zinc-800/80">
                        <span className="text-cyan-400 font-mono">
                          {modelGuidance.context_window_tokens >= 1000 
                            ? `${Math.round(modelGuidance.context_window_tokens / 1024)}k` 
                            : modelGuidance.context_window_tokens}
                        </span>
                        <span className="text-zinc-300">Context</span>
                      </div>
                    </div>

                    {modelGuidance.recommended_for && modelGuidance.recommended_for.length > 0 && (
                      <div className="pt-1">
                        <div className="text-[10px] text-zinc-400 mb-1">Recommended for:</div>
                        <div className="flex flex-wrap gap-1">
                          {modelGuidance.recommended_for.map((rec: string, idx: number) => (
                            <span key={idx} className="text-[10px] bg-zinc-900 text-zinc-300 px-1.5 py-0.5 rounded border border-zinc-800">
                              {rec}
                            </span>
                          ))}
                        </div>
                      </div>
                    )}

                    <div className="text-[9px] text-zinc-500 pt-1 flex items-center justify-between border-t border-zinc-800/60">
                      <span>{modelGuidance.is_local_capable ? 'Local Air-Gapped Capable' : 'Cloud Remote Gateway'}</span>
                      <span>100% Offline Static Catalog</span>
                    </div>
                  </div>
                )}
              </div>

              {/* API Key inputs */}
              <div className="space-y-3 pt-1">
                <div>
                  <label className="block text-xs font-semibold text-zinc-400 mb-1">OpenAI API Key</label>
                  <input
                    type="password"
                    value={openaiKey}
                    onChange={e => setOpenaiKey(e.target.value)}
                    placeholder="sk-..."
                    className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-white font-mono text-xs focus:outline-none focus:border-zinc-600"
                  />
                </div>

                <div>
                  <label className="block text-xs font-semibold text-zinc-400 mb-1">Anthropic API Key</label>
                  <input
                    type="password"
                    value={anthropicKey}
                    onChange={e => setAnthropicKey(e.target.value)}
                    placeholder="sk-ant-..."
                    className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-white font-mono text-xs focus:outline-none focus:border-zinc-600"
                  />
                </div>

                <div>
                  <label className="block text-xs font-semibold text-zinc-400 mb-1">Google Gemini API Key</label>
                  <input
                    type="password"
                    value={geminiKey}
                    onChange={e => setGeminiKey(e.target.value)}
                    placeholder="AIzaSy..."
                    className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-white font-mono text-xs focus:outline-none focus:border-zinc-600"
                  />
                </div>

                <div>
                  <label className="block text-xs font-semibold text-zinc-400 mb-1">OpenRouter API Key</label>
                  <input
                    type="password"
                    value={openrouterKey}
                    onChange={e => setOpenrouterKey(e.target.value)}
                    placeholder="sk-or-..."
                    className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-white font-mono text-xs focus:outline-none focus:border-zinc-600"
                  />
                </div>

                <div>
                  <label className="block text-xs font-semibold text-zinc-400 mb-1">Ollama Daemon Endpoint</label>
                  <input
                    type="text"
                    value={ollamaEndpoint}
                    onChange={e => setOllamaEndpoint(e.target.value)}
                    placeholder="http://localhost:11434"
                    className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-white font-mono text-xs focus:outline-none focus:border-zinc-600"
                  />
                </div>
              </div>
            </div>
          )}

          {/* TAB: Automations & Nightly Dreaming */}
          {activeTab === 'automations' && (
            <div className="space-y-5">
              {/* Nightly Dreaming Toggle & Memory Consolidation */}
              <div className="rounded-lg bg-zinc-900/60 border border-zinc-800 p-4 space-y-3">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="font-semibold text-sm text-white flex items-center space-x-2">
                      <span>🌙 Nightly Dreaming & Memory Consolidation</span>
                    </div>
                    <p className="text-xs text-zinc-400 mt-1">
                      Periodically distills daily conversational history and extracts durable rules into SQLite memory.
                    </p>
                  </div>
                  <label className="relative inline-flex items-center cursor-pointer">
                    <input
                      type="checkbox"
                      checked={nightlyDreamingEnabled}
                      onChange={e => setNightlyDreamingEnabled(e.target.checked)}
                      className="sr-only peer"
                    />
                    <div className="w-11 h-6 bg-zinc-800 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-blue-600"></div>
                  </label>
                </div>

                {/* Credit Protection Notice */}
                <div className="bg-amber-950/20 border border-amber-500/30 rounded p-2.5 text-xs text-amber-300 space-y-1">
                  <div className="font-bold">⚠️ Credit Protection Notice</div>
                  <p>
                    Consolidation is <strong>disabled by default</strong> to protect your API credits. You can point it to a free local Ollama model to run memory distillation completely free of charge.
                  </p>
                </div>

                <div className="flex items-center space-x-3 pt-1">
                  <div className="flex-1">
                    <label className="block text-[11px] text-zinc-400 mb-1">Consolidation Model</label>
                    <input
                      type="text"
                      value={dreamingModel}
                      onChange={e => setDreamingModel(e.target.value)}
                      placeholder="e.g. qwen2.5-coder:3b or gpt-4o-mini"
                      className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-white font-mono text-xs"
                    />
                  </div>
                  <div className="pt-5">
                    <button
                      onClick={handleTriggerDreamingNow}
                      disabled={isDreaming || !activeProjectId}
                      className="px-3 py-2 rounded bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white font-semibold text-xs transition flex items-center space-x-1"
                    >
                      <Play size={12} />
                      <span>{isDreaming ? 'Consolidating...' : 'Consolidate 24h Now'}</span>
                    </button>
                  </div>
                </div>

                {dreamResult && (
                  <div className="p-2 rounded bg-zinc-950 border border-zinc-800 text-xs font-mono text-zinc-300">
                    {dreamResult}
                  </div>
                )}
              </div>

              {/* Continuous Scheduled Automations */}
              <div className="rounded-lg bg-zinc-900/60 border border-zinc-800 p-4 space-y-3">
                <div className="font-semibold text-sm text-white">Continuous Scheduled Tasks</div>
                <p className="text-xs text-zinc-400">
                  Tasks that the engine executes on a recurring cron or heartbeat schedule.
                </p>

                {/* Add new schedule form */}
                <form onSubmit={handleCreateSchedule} className="p-3 rounded bg-zinc-950/60 border border-zinc-800 space-y-2 text-xs">
                  <div className="grid grid-cols-2 gap-2">
                    <input
                      type="text"
                      placeholder="Interval / Cron (e.g. '@every 2h', '0 2 * * *')"
                      value={newScheduleCron}
                      onChange={e => setNewScheduleCron(e.target.value)}
                      className="rounded bg-zinc-900 border border-zinc-800 p-2 text-white"
                    />
                    <input
                      type="text"
                      placeholder="Summary (e.g. 'Daily Security Scan')"
                      value={newScheduleSummary}
                      onChange={e => setNewScheduleSummary(e.target.value)}
                      className="rounded bg-zinc-900 border border-zinc-800 p-2 text-white"
                    />
                  </div>
                  <input
                    type="text"
                    placeholder="Action Prompt (instructions dispatched to agent inbox)"
                    value={newSchedulePrompt}
                    onChange={e => setNewSchedulePrompt(e.target.value)}
                    className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-white"
                  />
                  <button
                    type="submit"
                    disabled={!newScheduleCron.trim() || !newSchedulePrompt.trim()}
                    className="w-full py-1.5 rounded bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-white font-medium"
                  >
                    Add Scheduled Automation
                  </button>
                </form>

                {/* Scheduled Tasks List */}
                <div className="space-y-2 pt-2">
                  <div className="font-semibold text-xs text-zinc-400">Active Schedules ({scheduledTasks.length})</div>
                  {scheduledTasks.length === 0 ? (
                    <div className="text-xs text-zinc-500 italic p-3 bg-zinc-900/20 rounded">
                      No continuous schedules registered. Create one above or use <code>/schedule</code> in the chat!
                    </div>
                  ) : (
                    scheduledTasks.map(task => (
                      <div key={task.id} className="flex items-center justify-between p-3 rounded bg-zinc-900/40 border border-zinc-800 text-xs">
                        <div className="space-y-0.5 truncate max-w-md">
                          <div className="font-semibold text-white flex items-center space-x-2">
                            <span>{task.human_readable}</span>
                            <span className="text-[10px] font-mono bg-zinc-800 px-1.5 py-0.5 rounded text-zinc-400">{task.cron_expression}</span>
                          </div>
                          <div className="text-[11px] text-zinc-400 truncate">{task.action_prompt}</div>
                        </div>

                        <div className="flex items-center space-x-2">
                          <button
                            onClick={() => handleToggleSchedule(task.id, task.is_active)}
                            className={`px-2 py-1 rounded font-semibold text-[10px] ${
                              task.is_active
                                ? 'bg-emerald-900/40 text-emerald-300 border border-emerald-500/30'
                                : 'bg-zinc-800 text-zinc-500'
                            }`}
                          >
                            {task.is_active ? 'ACTIVE' : 'PAUSED'}
                          </button>
                          <button
                            onClick={() => handleDeleteSchedule(task.id)}
                            className="p-1.5 rounded text-zinc-500 hover:text-red-400 hover:bg-zinc-800"
                          >
                            <Trash2 size={13} />
                          </button>
                        </div>
                      </div>
                    ))
                  )}
                </div>
              </div>
            </div>
          )}

          {/* TAB: Telegram Gateway */}
          {activeTab === 'telegram' && (
            <div className="space-y-4">
              <div className="rounded-lg bg-zinc-900/60 border border-zinc-800 p-4 space-y-3">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="font-semibold text-sm text-white">Telegram Gateway (Headless Node)</div>
                    <p className="text-xs text-zinc-400 mt-1">
                      Allows running your Trans4mers node headlessly and controlling agents remotely via Telegram bot messages.
                    </p>
                  </div>
                  <label className="relative inline-flex items-center cursor-pointer">
                    <input
                      type="checkbox"
                      checked={telegramEnabled}
                      onChange={e => setTelegramEnabled(e.target.checked)}
                      className="sr-only peer"
                    />
                    <div className="w-11 h-6 bg-zinc-800 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-blue-600"></div>
                  </label>
                </div>

                <div className="space-y-3 pt-2">
                  <div>
                    <label className="block text-xs font-semibold text-zinc-400 mb-1">Telegram Bot Token</label>
                    <input
                      type="password"
                      value={telegramBotToken}
                      onChange={e => setTelegramBotToken(e.target.value)}
                      placeholder="123456789:ABCdefGhIJKlmNoPQRsTUVwxyZ"
                      className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-white font-mono text-xs"
                    />
                  </div>

                  <div>
                    <label className="block text-xs font-semibold text-zinc-400 mb-1">
                      Allowed Chat IDs (Whitelist, comma-separated)
                    </label>
                    <input
                      type="text"
                      value={telegramAllowedChatIds}
                      onChange={e => setTelegramAllowedChatIds(e.target.value)}
                      placeholder="e.g. 123456789, 987654321 (leave empty to allow any authenticated user)"
                      className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-white font-mono text-xs"
                    />
                  </div>

                  <div className="flex items-center space-x-3 pt-2">
                    <button
                      onClick={handleTestTelegram}
                      disabled={isTestingTelegram || !telegramBotToken.trim()}
                      className="px-3 py-1.5 rounded bg-zinc-800 hover:bg-zinc-700 disabled:opacity-50 text-white font-semibold text-xs transition"
                    >
                      {isTestingTelegram ? 'Testing...' : 'Test Bot Connection'}
                    </button>

                    <button
                      onClick={handleSaveTelegram}
                      className="px-4 py-1.5 rounded bg-blue-600 hover:bg-blue-500 text-white font-semibold text-xs transition"
                    >
                      Save Gateway Settings
                    </button>
                  </div>

                  {telegramTestStatus && (
                    <div className="p-2.5 rounded bg-zinc-950 border border-zinc-800 text-xs font-mono text-zinc-300">
                      {telegramTestStatus}
                    </div>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* TAB: Hardware & Performance Profile */}
          {activeTab === 'performance' && (
            <div className="space-y-4">
              <div className="rounded-lg bg-zinc-900/60 border border-zinc-800 p-4 space-y-2">
                <div className="font-semibold text-white">Performance Profile</div>
                <p className="text-xs text-zinc-400">
                  Select your system profile. Low-Spec Local Mode automatically compresses context, optimizes concurrency, and preserves VRAM for 8B-14B models.
                </p>
                <div className="grid grid-cols-2 gap-3 pt-2">
                  {[
                    { id: 'LowSpecLocal', label: 'Low-Spec Local', desc: '8B/14B models, 8-16GB RAM. Aggressive context compaction & 1 concurrent LLM slot.' },
                    { id: 'HighSpecCloud', label: 'High-Spec / Cloud', desc: '32B/70B models or BYOK Cloud APIs (OpenAI/Anthropic/Gemini). Max concurrency.' },
                  ].map(p => (
                    <div
                      key={p.id}
                      onClick={() => setPerfProfile(prev => ({ ...prev, mode: p.id as any }))}
                      className={`cursor-pointer p-3 rounded-lg border text-left transition ${
                        perfProfile.mode === p.id
                          ? 'border-cyan-500 bg-cyan-950/20 text-white'
                          : 'border-zinc-800 bg-zinc-900/30 text-zinc-400 hover:border-zinc-700'
                      }`}
                    >
                      <div className="font-bold text-xs">{p.label}</div>
                      <div className="text-[11px] text-zinc-500 mt-1">{p.desc}</div>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          )}

          {/* TAB: Context Compaction */}
          {activeTab === 'compaction' && (
            <div className="space-y-4">
              <div className="flex items-center justify-between p-3 rounded bg-zinc-900/40 border border-zinc-800">
                <div>
                  <div className="font-semibold text-xs text-white">Enable Context Compaction</div>
                  <div className="text-[11px] text-zinc-400">Automatically summarize historical ReAct steps to save tokens</div>
                </div>
                <input
                  type="checkbox"
                  checked={compaction.enabled}
                  onChange={e => setCompaction(prev => ({ ...prev, enabled: e.target.checked }))}
                  className="rounded bg-zinc-800 border-zinc-700 h-4 w-4"
                />
              </div>

              <div>
                <label className="block text-xs font-semibold text-zinc-400 mb-1">
                  Trigger Threshold: {Math.round(compaction.trigger_threshold_pct * 100)}% of Context Window
                </label>
                <input
                  type="range"
                  min="0.5"
                  max="0.95"
                  step="0.05"
                  value={compaction.trigger_threshold_pct}
                  onChange={e => setCompaction(prev => ({ ...prev, trigger_threshold_pct: parseFloat(e.target.value) }))}
                  className="w-full accent-cyan-400"
                />
              </div>
            </div>
          )}

          {/* TAB: Diff Review & Trust */}
          {activeTab === 'diff' && (
            <div className="space-y-4">
              <div className="rounded-lg bg-zinc-900/60 border border-zinc-800 p-4 space-y-3">
                <div className="font-semibold text-white">Human-in-the-Loop Diff Review Policy</div>
                <p className="text-xs text-zinc-400">
                  Configure when the system requires explicit human approval before applying file modifications.
                </p>

                <div className="flex items-center justify-between p-2 rounded bg-zinc-900/30">
                  <span className="text-xs">Auto-approve small safe file modifications</span>
                  <input
                    type="checkbox"
                    checked={diffReview.auto_approve_small_files}
                    onChange={e => setDiffReview(prev => ({ ...prev, auto_approve_small_files: e.target.checked }))}
                    className="rounded bg-zinc-800 border-zinc-700 h-4 w-4"
                  />
                </div>

                <div className="flex items-center justify-between p-2 rounded bg-zinc-900/30">
                  <span className="text-xs">Auto-approve clean non-destructive shell commands</span>
                  <input
                    type="checkbox"
                    checked={diffReview.auto_approve_clean_commands}
                    onChange={e => setDiffReview(prev => ({ ...prev, auto_approve_clean_commands: e.target.checked }))}
                    className="rounded bg-zinc-800 border-zinc-700 h-4 w-4"
                  />
                </div>
              </div>
            </div>
          )}

          {/* TAB: Cost Guard */}
          {activeTab === 'cost' && (
            <div className="space-y-4">
              <div className="rounded-lg bg-zinc-900/60 border border-zinc-800 p-4 space-y-3">
                <div className="font-semibold text-white">Autonomous Cost Guard</div>
                <p className="text-xs text-zinc-400">
                  Hard spend limits protecting against recursive runaways and uncontrolled token usage.
                </p>

                {costData && (
                  <div className="grid grid-cols-2 gap-3 p-3 bg-zinc-950/60 rounded border border-zinc-800 text-xs">
                    <div>
                      <span className="text-zinc-400">Today's Spend:</span>{' '}
                      <span className="font-mono font-bold text-emerald-400">${costData.daily_usd.toFixed(4)}</span>
                    </div>
                    <div>
                      <span className="text-zinc-400">Month's Spend:</span>{' '}
                      <span className="font-mono font-bold text-emerald-400">${costData.monthly_usd.toFixed(4)}</span>
                    </div>
                  </div>
                )}

                <div className="grid grid-cols-2 gap-3 pt-2">
                  <div>
                    <label className="block text-xs font-semibold text-zinc-400 mb-1">Monthly Ceiling ($ USD)</label>
                    <input
                      type="number"
                      value={monthlyCeiling}
                      onChange={e => setMonthlyCeiling(e.target.value)}
                      className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-white text-xs"
                    />
                  </div>

                  <div>
                    <label className="block text-xs font-semibold text-zinc-400 mb-1">Daily Ceiling ($ USD)</label>
                    <input
                      type="number"
                      value={dailyCeiling}
                      onChange={e => setDailyCeiling(e.target.value)}
                      className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-white text-xs"
                    />
                  </div>
                </div>

                <div className="flex items-center justify-between p-2 rounded bg-zinc-900/30 mt-2">
                  <span className="text-xs">Hard Block (Reject requests when budget exceeded)</span>
                  <input
                    type="checkbox"
                    checked={hardBlock}
                    onChange={e => setHardBlock(e.target.checked)}
                    className="rounded bg-zinc-800 border-zinc-700 h-4 w-4"
                  />
                </div>
              </div>
            </div>
          )}

          {/* TAB: MCP Registry */}
          {activeTab === 'mcp' && (
            <div className="space-y-4">
              <div className="rounded-lg bg-zinc-900/60 border border-zinc-800 p-4 space-y-3">
                <div className="flex items-center justify-between">
                  <div className="font-semibold text-white">Register External MCP Server</div>
                  <span
                    className={`text-[10px] font-mono px-2 py-0.5 rounded border ${
                      newMcpCommand.trim().startsWith('http://') || newMcpCommand.trim().startsWith('https://')
                        ? 'border-cyan-500/40 bg-cyan-950/40 text-cyan-300'
                        : 'border-zinc-700 bg-zinc-800/40 text-zinc-400'
                    }`}
                  >
                    Transport: {newMcpCommand.trim().startsWith('http://') || newMcpCommand.trim().startsWith('https://') ? 'Remote Streamable HTTP (SSE)' : 'Local Stdio Subprocess'}
                  </span>
                </div>
                <div className="grid grid-cols-2 gap-3">
                  <input
                    type="text"
                    placeholder="Server Name (e.g. github, remote-db)"
                    value={newMcpName}
                    onChange={e => setNewMcpName(e.target.value)}
                    className="rounded bg-zinc-900 border border-zinc-800 p-2 text-xs text-white"
                  />
                  <input
                    type="text"
                    placeholder="Command or URL (http://... or npx -y ...)"
                    value={newMcpCommand}
                    onChange={e => setNewMcpCommand(e.target.value)}
                    className="rounded bg-zinc-900 border border-zinc-800 p-2 text-xs text-white font-mono"
                  />
                </div>
                <div className="grid grid-cols-1 gap-3">
                  <input
                    type="password"
                    placeholder="Auth Bearer Token (optional - stored securely in OS keyring, never plaintext)"
                    value={newMcpToken}
                    onChange={e => setNewMcpToken(e.target.value)}
                    className="rounded bg-zinc-900 border border-zinc-800 p-2 text-xs text-white font-mono"
                  />
                </div>
                <div className="flex justify-end">
                  <button
                    type="button"
                    onClick={handleAddMcp}
                    className="px-3 py-1.5 rounded bg-brand-primary hover:bg-brand-primary/80 text-white font-semibold text-xs"
                  >
                    Register Server
                  </button>
                </div>
              </div>

              <div className="space-y-2">
                <div className="font-semibold text-xs text-zinc-400">Registered Servers ({mcpServers.length})</div>
                {mcpServers.length === 0 ? (
                  <div className="text-xs text-zinc-500 italic p-3 bg-zinc-900/20 rounded">No external MCP servers registered.</div>
                ) : (
                  mcpServers.map(s => {
                    const isHttp = s.command.startsWith('http://') || s.command.startsWith('https://');
                    const hasKeyring = mcpAuthStatus[s.name];
                    return (
                      <div key={s.name} className="flex items-center justify-between p-3 rounded bg-zinc-900/40 border border-zinc-800">
                        <div className="space-y-1">
                          <div className="flex items-center space-x-2">
                            <span className="font-semibold text-xs text-white">{s.name}</span>
                            <span
                              className={`text-[9px] font-mono px-1.5 py-0.5 rounded ${
                                isHttp
                                  ? 'bg-cyan-950/60 text-cyan-300 border border-cyan-800/40'
                                  : 'bg-zinc-800/80 text-zinc-400 border border-zinc-700/40'
                              }`}
                            >
                              {isHttp ? 'http/sse' : 'stdio'}
                            </span>
                            {hasKeyring && (
                              <span className="text-[9px] font-mono px-1.5 py-0.5 rounded bg-emerald-950/60 text-emerald-300 border border-emerald-800/40">
                                Keyring Auth: Set
                              </span>
                            )}
                          </div>
                          <div className="text-[11px] font-mono text-zinc-400 truncate max-w-md">{s.command}</div>
                        </div>
                        <div className="flex items-center space-x-2">
                          <button
                            type="button"
                            onClick={() => setInspectingServer({ name: s.name, command: s.command })}
                            className="flex items-center space-x-1 px-2 py-1 rounded bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-xs font-medium border border-zinc-700 transition"
                            title="Inspect JSON-RPC traffic and launch MCP inspector"
                          >
                            <Activity size={12} className="text-cyan-400" />
                            <span>Inspect</span>
                          </button>
                          <button
                            onClick={() => handleToggleMcpApproval(s.name, s.approved)}
                            className={`px-2 py-1 rounded text-xs font-semibold ${
                              s.approved
                                ? 'bg-emerald-900/40 text-emerald-300 border border-emerald-500/30'
                                : 'bg-yellow-900/40 text-yellow-300 border border-yellow-500/30'
                            }`}
                          >
                            {s.approved ? 'Approved' : 'Pending'}
                          </button>
                          <button
                            onClick={() => handleDeleteMcp(s.name)}
                            className="p-1 rounded text-zinc-500 hover:text-red-400 hover:bg-zinc-800"
                          >
                            <X size={14} />
                          </button>
                        </div>
                      </div>
                    );
                  })
                )}
              </div>
            </div>
          )}

          {/* TAB 9: Langfuse Observability */}
          {activeTab === 'langfuse' && (
            <div className="space-y-4">
              {/* Status Header Banner */}
              <div className="flex items-center justify-between p-3.5 rounded-lg bg-zinc-900/60 border border-zinc-800">
                <div className="flex items-center space-x-3">
                  <div className={`p-2 rounded-lg ${
                    langfuseEnabled && langfuseHasPublicKey && langfuseHasSecretKey
                      ? 'bg-emerald-950/40 text-emerald-400 border border-emerald-500/30'
                      : langfuseEnabled
                      ? 'bg-amber-950/40 text-amber-400 border border-amber-500/30'
                      : 'bg-zinc-800 text-zinc-400 border border-zinc-700'
                  }`}>
                    <Activity size={18} />
                  </div>
                  <div>
                    <div className="text-xs font-bold text-white flex items-center space-x-2">
                      <span>Langfuse Telemetry Gateway</span>
                      <span className={`px-2 py-0.5 text-[10px] rounded-full font-semibold ${
                        langfuseEnabled && langfuseHasPublicKey && langfuseHasSecretKey
                          ? 'bg-emerald-500/20 text-emerald-400 border border-emerald-500/30'
                          : langfuseEnabled
                          ? 'bg-amber-500/20 text-amber-400 border border-amber-500/30'
                          : 'bg-zinc-800 text-zinc-400 border border-zinc-700'
                      }`}>
                        {langfuseEnabled && langfuseHasPublicKey && langfuseHasSecretKey
                          ? 'Active • Streaming Traces'
                          : langfuseEnabled
                          ? 'Enabled • Missing Keyring Credentials'
                          : 'Disabled • Zero Egress'}
                      </span>
                    </div>
                    <div className="text-[11px] text-zinc-400 mt-0.5">
                      Mirror sovereign agent traces, token usage, tool spans, and RAG retrieval to Langfuse.
                    </div>
                  </div>
                </div>

                <button
                  type="button"
                  onClick={() => setLangfuseEnabled(!langfuseEnabled)}
                  className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition ${
                    langfuseEnabled
                      ? 'bg-emerald-600 hover:bg-emerald-500 text-white'
                      : 'bg-zinc-800 hover:bg-zinc-700 text-zinc-300'
                  }`}
                >
                  {langfuseEnabled ? 'Enabled' : 'Disabled'}
                </button>
              </div>

              {/* Invariant Note */}
              <div className="rounded-lg bg-zinc-900/30 border border-zinc-800 p-3 text-xs text-zinc-400 space-y-1">
                <div className="font-semibold text-zinc-300 flex items-center space-x-1.5">
                  <ShieldCheck size={14} className="text-emerald-400" />
                  <span>Sovereign Zero-Egress Invariant</span>
                </div>
                <p className="text-[11px] text-zinc-400 leading-relaxed">
                  When disabled or unset, Trans4mers executes with strictly zero network calls to Langfuse.
                  Traces mirror persisted SQLite database metrics directly, ensuring 100% token parity with your local cost dashboard. Local Ollama executions remain $0.00.
                </p>
              </div>

              {/* Form Inputs */}
              <div className="space-y-3 rounded-lg bg-zinc-900/20 border border-zinc-800/80 p-4">
                <div>
                  <label className="block text-xs font-semibold text-zinc-400 mb-1">
                    Langfuse Endpoint Host URL
                  </label>
                  <input
                    type="text"
                    value={langfuseHost}
                    onChange={e => setLangfuseHost(e.target.value)}
                    placeholder="http://localhost:3000"
                    className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-xs text-white focus:outline-none focus:border-zinc-600 font-mono"
                  />
                  <div className="text-[10px] text-zinc-500 mt-1">
                    Supports self-hosted docker instances or Langfuse Cloud (<code className="text-zinc-400">https://cloud.langfuse.com</code>).
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <div className="flex items-center justify-between mb-1">
                      <label className="text-xs font-semibold text-zinc-400">Public Key</label>
                      <span className={`text-[10px] px-1.5 py-0.2 rounded font-mono ${
                        langfuseHasPublicKey ? 'bg-emerald-950 text-emerald-400 border border-emerald-800' : 'text-zinc-500'
                      }`}>
                        {langfuseHasPublicKey ? 'OS Keyring' : 'Not set'}
                      </span>
                    </div>
                    <input
                      type="password"
                      value={langfusePublicKey}
                      onChange={e => setLangfusePublicKey(e.target.value)}
                      placeholder="pk-lf-..."
                      className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-xs text-white focus:outline-none focus:border-zinc-600 font-mono"
                    />
                  </div>

                  <div>
                    <div className="flex items-center justify-between mb-1">
                      <label className="text-xs font-semibold text-zinc-400">Secret Key</label>
                      <span className={`text-[10px] px-1.5 py-0.2 rounded font-mono ${
                        langfuseHasSecretKey ? 'bg-emerald-950 text-emerald-400 border border-emerald-800' : 'text-zinc-500'
                      }`}>
                        {langfuseHasSecretKey ? 'OS Keyring' : 'Not set'}
                      </span>
                    </div>
                    <input
                      type="password"
                      value={langfuseSecretKey}
                      onChange={e => setLangfuseSecretKey(e.target.value)}
                      placeholder="sk-lf-..."
                      className="w-full rounded bg-zinc-900 border border-zinc-800 p-2 text-xs text-white focus:outline-none focus:border-zinc-600 font-mono"
                    />
                  </div>
                </div>

                {/* Prompt Capture Privacy Toggle */}
                <div className="pt-2 border-t border-zinc-800/60 flex items-center justify-between">
                  <div className="space-y-0.5">
                    <div className="text-xs font-semibold text-zinc-300">Prompt & Completion Capture</div>
                    <div className="text-[11px] text-zinc-500 max-w-md">
                      When off (default), traces contain only metadata, token counts, and tool names. When on, prompts are scrubbed of API keys before transmission.
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => setLangfuseCapturePrompts(!langfuseCapturePrompts)}
                    className={`px-3 py-1 rounded text-xs font-semibold transition ${
                      langfuseCapturePrompts
                        ? 'bg-amber-600 hover:bg-amber-500 text-white'
                        : 'bg-zinc-800 hover:bg-zinc-700 text-zinc-400'
                    }`}
                  >
                    {langfuseCapturePrompts ? 'Capturing (Redacted)' : 'Metadata Only (Default)'}
                  </button>
                </div>
              </div>

              {/* Action Buttons: Test Connection & Save */}
              <div className="flex items-center justify-between pt-2">
                <button
                  type="button"
                  onClick={handleTestLangfuse}
                  disabled={isTestingLangfuse || !langfuseHost.trim()}
                  className="flex items-center space-x-1.5 px-3 py-1.5 rounded bg-zinc-800 hover:bg-zinc-700 text-zinc-300 font-semibold text-xs transition disabled:opacity-50"
                >
                  <RefreshCw size={12} className={isTestingLangfuse ? 'animate-spin' : ''} />
                  <span>{isTestingLangfuse ? 'Testing...' : 'Test Connection'}</span>
                </button>

                {langfuseTestStatus && (
                  <div className={`text-xs px-3 py-1 rounded font-mono ${
                    langfuseTestStatus.startsWith('✓')
                      ? 'text-emerald-400 bg-emerald-950/30 border border-emerald-500/20'
                      : 'text-red-400 bg-red-950/30 border border-red-500/20'
                  }`}>
                    {langfuseTestStatus}
                  </div>
                )}
              </div>
            </div>
          )}

          {/* GitHub Sync Tab */}
          {activeTab === 'github' && (
            <div className="space-y-6">
              <div className="flex items-center justify-between">
                <div>
                  <h3 className="text-sm font-semibold text-white">GitHub Integration & Sovereignty</h3>
                  <p className="text-xs text-zinc-400">
                    Opt-in synchronization for importing GitHub issues into delegated Boss Agent tasks and reviewing PR diffs with file/line provenance.
                  </p>
                </div>
                <div className={`px-2.5 py-1 rounded text-xs font-semibold flex items-center space-x-1.5 ${
                  githubConfigured
                    ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                    : 'bg-zinc-800 text-zinc-400'
                }`}>
                  <span className={`w-2 h-2 rounded-full ${githubConfigured ? 'bg-emerald-400 animate-pulse' : 'bg-zinc-600'}`} />
                  <span>{githubConfigured ? 'Keyring Secured' : 'No Token'}</span>
                </div>
              </div>

              {/* Security Invariant Note */}
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-3 text-xs text-zinc-300 space-y-1">
                <div className="font-semibold text-amber-400 flex items-center space-x-1.5">
                  <ShieldCheck size={14} />
                  <span>Sovereign Security Invariant</span>
                </div>
                <p className="text-zinc-400">
                  Your Personal Access Token (PAT) is stored exclusively in your local operating system's credential vault (OS Keyring). It is never written to SQLite, config files, or step event logs.
                </p>
              </div>

              {/* Token Configuration */}
              <div className="space-y-3">
                <label className="block text-xs font-medium text-zinc-300">
                  Personal Access Token (classic `ghp_...` or fine-grained)
                </label>
                <div className="flex space-x-2">
                  <input
                    type="password"
                    placeholder={githubConfigured ? '••••••••••••••••••••••••••••••••' : 'Enter GitHub Personal Access Token...'}
                    value={githubPat}
                    onChange={e => setGithubPat(e.target.value)}
                    className="flex-1 px-3 py-2 rounded-lg bg-zinc-900 border border-zinc-800 text-zinc-100 font-mono text-xs focus:outline-none focus:border-brand-primary"
                  />
                  <button
                    onClick={handleSaveGithubPat}
                    disabled={!githubPat.trim() || githubPat === '********'}
                    className="px-4 py-2 rounded-lg bg-brand-primary hover:bg-brand-primary/80 text-white font-medium text-xs transition disabled:opacity-50"
                  >
                    Save Token
                  </button>
                  {githubConfigured && (
                    <button
                      onClick={handleDeleteGithubPat}
                      className="px-3 py-2 rounded-lg bg-red-950/40 hover:bg-red-900/60 text-red-300 border border-red-800/40 text-xs transition"
                      title="Delete from OS Keyring"
                    >
                      <Trash2 size={14} />
                    </button>
                  )}
                </div>
                {githubMasked && (
                  <p className="text-[11px] text-zinc-500 font-mono">
                    Active Token in OS Keyring: <span className="text-zinc-300">{githubMasked}</span>
                  </p>
                )}
              </div>

              {/* Direct Issue Importer */}
              <div className="border-t border-zinc-800 pt-5 space-y-3">
                <h4 className="text-xs font-semibold text-white">Import Issue into Delegated Boss Task</h4>
                <p className="text-xs text-zinc-400">
                  Provide an issue URL or short reference (e.g. <code className="text-zinc-300">owner/repo#123</code> or <code className="text-zinc-300">https://github.com/facebook/react/issues/1000</code>).
                </p>
                <div className="flex space-x-2">
                  <input
                    type="text"
                    placeholder="https://github.com/owner/repo/issues/123 or owner/repo#123"
                    value={githubImportUrl}
                    onChange={e => setGithubImportUrl(e.target.value)}
                    className="flex-1 px-3 py-2 rounded-lg bg-zinc-900 border border-zinc-800 text-zinc-100 font-mono text-xs focus:outline-none focus:border-brand-primary"
                  />
                  <button
                    onClick={handleImportGithubIssue}
                    disabled={githubImporting || !githubImportUrl.trim()}
                    className="px-4 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-100 font-medium text-xs transition disabled:opacity-50 flex items-center space-x-1.5"
                  >
                    <RefreshCw size={13} className={githubImporting ? 'animate-spin' : ''} />
                    <span>{githubImporting ? 'Importing...' : 'Import & Delegate'}</span>
                  </button>
                </div>
                {githubImportMsg && (
                  <div className={`text-xs px-3 py-2 rounded border font-sans ${
                    githubImportMsg.type === 'success'
                      ? 'bg-emerald-950/40 text-emerald-300 border-emerald-500/30'
                      : 'bg-red-950/40 text-red-300 border-red-500/30'
                  }`}>
                    {githubImportMsg.text}
                  </div>
                )}
              </div>
            </div>
          )}
        </div>

        {/* Modal Footer */}
        <div className="px-6 py-4 border-t border-zinc-800 bg-zinc-900/60 flex items-center justify-between">
          <div className="text-xs text-emerald-400 flex items-center space-x-1.5">
            {savedSuccess && (
              <>
                <Check size={14} />
                <span>Configuration saved successfully!</span>
              </>
            )}
          </div>

          <div className="flex items-center space-x-3">
            <button
              onClick={onClose}
              className="px-4 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-300 font-medium text-xs transition"
            >
              Close
            </button>
            <button
              onClick={handleSave}
              disabled={saving}
              className="flex items-center space-x-1.5 px-5 py-2 rounded-lg bg-brand-primary hover:bg-brand-primary/80 text-white font-semibold text-xs transition disabled:opacity-50"
            >
              <Check size={14} />
              <span>{saving ? 'Saving...' : 'Save Changes'}</span>
            </button>
          </div>
        </div>
      </div>

      {inspectingServer && (
        <McpInspectorModal
          isOpen={!!inspectingServer}
          serverName={inspectingServer.name}
          serverCommand={inspectingServer.command}
          onClose={() => setInspectingServer(null)}
        />
      )}
    </div>
  );
}
