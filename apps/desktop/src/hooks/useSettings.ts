import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface CompactionConfig {
  enabled: boolean;
  trigger_threshold_pct: number;
  preserve_recent_messages: number;
  use_llm_summary: boolean;
}

export interface DiffReviewConfig {
  auto_approve_small_files: boolean;
  auto_approve_clean_commands: boolean;
  force_review_secret_patterns: string[];
}

export interface PerformanceProfile {
  mode: 'LowSpecLocal' | 'HighSpecCloud';
  max_active_llm_slots: number;
  symbolic_short_term_enabled: boolean;
  idle_only_background_extraction: boolean;
}

export interface FeatureToggles {
  nightly_dreaming_enabled: boolean;
  dreaming_model: string | null;
  dynamic_model_scan_enabled: boolean;
  human_in_the_loop_sync: boolean;
}

export interface AppSettings {
  theme: 'light' | 'dark';
  llmProvider: string;
  openaiApiKey: string;
  anthropicApiKey: string;
  geminiApiKey: string;
  openrouterApiKey: string;
  ollamaEndpoint: string;
  features: FeatureToggles;
  compaction: CompactionConfig;
  diffReview: DiffReviewConfig;
  performanceProfile: PerformanceProfile;
}

export function useSettings() {
  const [settings, setSettings] = useState<AppSettings>({
    theme: 'dark',
    llmProvider: 'ollama',
    openaiApiKey: '',
    anthropicApiKey: '',
    geminiApiKey: '',
    openrouterApiKey: '',
    ollamaEndpoint: 'http://127.0.0.1:11434',
    features: {
      nightly_dreaming_enabled: false,
      dreaming_model: null,
      dynamic_model_scan_enabled: true,
      human_in_the_loop_sync: true,
    },
    compaction: {
      enabled: true,
      trigger_threshold_pct: 0.75,
      preserve_recent_messages: 6,
      use_llm_summary: true,
    },
    diffReview: {
      auto_approve_small_files: true,
      auto_approve_clean_commands: true,
      force_review_secret_patterns: [],
    },
    performanceProfile: {
      mode: 'LowSpecLocal',
      max_active_llm_slots: 1,
      symbolic_short_term_enabled: true,
      idle_only_background_extraction: true,
    },
  });

  const [loading, setLoading] = useState(true);

  const loadSettings = async () => {
    try {
      const res = await invoke<{
        provider: string;
        keys: { provider: string; is_set: boolean }[];
        compaction: CompactionConfig;
        diff_review: DiffReviewConfig;
        performance_profile: PerformanceProfile;
        features?: FeatureToggles;
        ollama_endpoint?: string;
      }>('get_settings');

      setSettings(prev => ({
        ...prev,
        llmProvider: res.provider,
        openaiApiKey: res.keys.find(k => k.provider === 'openai')?.is_set ? '********' : '',
        anthropicApiKey: res.keys.find(k => k.provider === 'anthropic')?.is_set ? '********' : '',
        geminiApiKey: res.keys.find(k => k.provider === 'google' || k.provider === 'gemini')?.is_set ? '********' : '',
        openrouterApiKey: res.keys.find(k => k.provider === 'openrouter')?.is_set ? '********' : '',
        ollamaEndpoint: res.ollama_endpoint || prev.ollamaEndpoint,
        features: res.features || prev.features,
        compaction: res.compaction || prev.compaction,
        diffReview: res.diff_review || prev.diffReview,
        performanceProfile: res.performance_profile || prev.performanceProfile,
      }));
    } catch (err) {
      console.error('Failed to load settings:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadSettings();
  }, []);

  const saveSettings = async (newSettings: Partial<AppSettings>) => {
    const tasks: Promise<unknown>[] = [];

    if (newSettings.llmProvider) {
      tasks.push(invoke('set_default_provider', { provider: newSettings.llmProvider }));
    }
    if (newSettings.openaiApiKey !== undefined && newSettings.openaiApiKey !== '********') {
      tasks.push(invoke('update_settings', { provider: 'openai', apiKey: newSettings.openaiApiKey }));
    }
    if (newSettings.anthropicApiKey !== undefined && newSettings.anthropicApiKey !== '********') {
      tasks.push(invoke('update_settings', { provider: 'anthropic', apiKey: newSettings.anthropicApiKey }));
    }
    if (newSettings.geminiApiKey !== undefined && newSettings.geminiApiKey !== '********') {
      tasks.push(invoke('update_settings', { provider: 'google', apiKey: newSettings.geminiApiKey }));
    }
    if (newSettings.openrouterApiKey !== undefined && newSettings.openrouterApiKey !== '********') {
      tasks.push(invoke('update_settings', { provider: 'openrouter', apiKey: newSettings.openrouterApiKey }));
    }
    if (newSettings.ollamaEndpoint) {
      tasks.push(invoke('update_provider_endpoint', { provider: 'ollama', endpoint: newSettings.ollamaEndpoint }));
    }
    if (newSettings.features) {
      tasks.push(invoke('update_feature_toggles', { features: newSettings.features }));
    }
    if (newSettings.compaction) {
      tasks.push(invoke('update_compaction_config', { config: newSettings.compaction }));
    }
    if (newSettings.diffReview) {
      tasks.push(invoke('update_diff_review_config', { config: newSettings.diffReview }));
    }
    if (newSettings.performanceProfile) {
      tasks.push(invoke('update_performance_profile', { profile: newSettings.performanceProfile }));
    }

    try {
      await Promise.all(tasks);
      await loadSettings();
    } catch (err) {
      console.error('Failed to save settings:', err);
      await loadSettings();
      throw err;
    }
  };

  return { settings, saveSettings, reloadSettings: loadSettings, loading };
}
