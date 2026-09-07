import React, { useState, useEffect } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';

interface SetupWizardProps {
  onComplete: () => void;
}

export default function SetupWizard({ onComplete }: SetupWizardProps) {
  const [provider, setProvider] = useState<'openai' | 'ollama'>('ollama');
  const [apiKey, setApiKey] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const isRunningInTauri = typeof window !== 'undefined' && (isTauri() || (window as any).__TAURI_INTERNALS__ !== undefined);

  useEffect(() => {
    if (!isRunningInTauri) {
      setError("Running in external web browser. Native Tauri IPC is active inside the Trans4mers desktop window on your Windows taskbar. Please switch to the Trans4mers desktop app window.");
      return;
    }
    // Check if settings are already configured
    invoke<{ provider: string; keys: Array<{ provider: string; is_set: boolean }> }>('get_settings')
      .then(settings => {
        const hasKey = settings.keys?.some(k => k.is_set && (k.provider === 'openai' || k.provider === 'anthropic'));
        if (settings.provider === 'ollama' || hasKey) {
          onComplete();
        }
      })
      .catch(console.error);
  }, [onComplete, isRunningInTauri]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setLoading(true);

    try {
      if (!isRunningInTauri) {
        throw new Error("Trans4mers is a native desktop application. Native IPC commands require running inside the Trans4mers desktop window. Please switch to the Trans4mers window on your taskbar.");
      }

      if (provider === 'openai' && !apiKey) {
        throw new Error('API Key is required for OpenAI');
      }

      let testModel = provider === 'ollama' ? '' : 'gpt-3.5-turbo';
      try {
        const models = await invoke<string[]>('list_available_models', {
          name: provider,
          apiKey: provider === 'openai' ? apiKey : null,
          endpoint: provider === 'ollama' ? 'http://localhost:11434' : null,
        });
        if (models && models.length > 0) {
          testModel = models[0];
        }
      } catch (err) {
        console.warn('Could not list models for provider', err);
      }

      // Test connection
      const success = await invoke<boolean>('test_provider_connection', { 
        name: provider, 
        endpoint: provider === 'ollama' ? 'http://localhost:11434' : 'https://api.openai.com/v1', 
        model: testModel || null, 
        apiKey: provider === 'openai' ? apiKey : null 
      });

      if (!success) {
        throw new Error('Connection test failed. Check your API key or Ollama daemon.');
      }

      // Update settings
      await invoke('update_settings', { provider, apiKey: provider === 'openai' ? apiKey : 'local' });
      
      onComplete();
    } catch (err: any) {
      setError(err.message || 'An unexpected error occurred');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-bg-base border border-gray-800 rounded-lg p-6 w-full max-w-md shadow-xl">
        <h2 className="text-2xl font-bold text-white mb-2">Welcome to Trans4mers</h2>
        <p className="text-gray-400 mb-6 text-sm">Please configure your LLM provider to continue.</p>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-300 mb-1">Provider</label>
            <select 
              className="w-full bg-[#1e1e1e] border border-gray-700 text-white rounded p-2 focus:border-brand-primary outline-none"
              value={provider}
              onChange={(e) => setProvider(e.target.value as 'openai' | 'ollama')}
            >
              <option value="openai">OpenAI</option>
              <option value="ollama">Ollama (Local)</option>
            </select>
          </div>

          {provider === 'openai' && (
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-1">API Key</label>
              <input 
                type="password"
                className="w-full bg-[#1e1e1e] border border-gray-700 text-white rounded p-2 focus:border-brand-primary outline-none"
                placeholder="sk-..."
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
              />
              <p className="text-xs text-gray-500 mt-1">Stored securely in your OS native keyring.</p>
            </div>
          )}

          {error && <div className="text-destructive text-sm bg-destructive/10 p-2 rounded">{error}</div>}

          <button 
            type="submit"
            disabled={loading}
            className="w-full bg-brand-primary text-white rounded py-2 font-medium hover:bg-brand-primary/90 disabled:opacity-50 transition-colors"
          >
            {loading ? 'Verifying...' : 'Save & Continue'}
          </button>
        </form>
      </div>
    </div>
  );
}
