import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { Download, Info, Folder, RotateCcw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

export function SetupOverviewStep() {
  const { goNext, modelsFolder, setModelsFolder } = useOnboarding();
  const [recommendedModel, setRecommendedModel] = useState<string>('gemma3:1b');
  const [modelSize, setModelSize] = useState<string>('~806 MB');
  const [isMac, setIsMac] = useState(false);
  const [defaultFolder, setDefaultFolder] = useState<string>('');
  const [displayFolder, setDisplayFolder] = useState<string>('');

  // Fetch recommended model and default folder on mount
  useEffect(() => {
    const fetchRecommendedModel = async () => {
      try {
        const model = await invoke<string>('builtin_ai_get_recommended_model');
        setRecommendedModel(model);
        setModelSize(model === 'gemma3:4b' ? '~2.5 GB' : '~806 MB');
      } catch (error) {
        console.error('Failed to get recommended model:', error);
      }
    };
    fetchRecommendedModel();

    const fetchDefaultFolder = async () => {
      try {
        const defFolder = await invoke<string>('get_default_models_folder');
        setDefaultFolder(defFolder);
        setDisplayFolder(modelsFolder || defFolder);
      } catch (error) {
        console.error('Failed to get default models folder:', error);
      }
    };
    fetchDefaultFolder();

    const checkPlatform = async () => {
      try {
        const { platform } = await import('@tauri-apps/plugin-os');
        setIsMac(platform() === 'macos');
      } catch (e) {
        setIsMac(navigator.userAgent.includes('Mac'));
      }
    };
    checkPlatform();
  }, []);

  // Sync display folder when modelsFolder changes externally
  useEffect(() => {
    if (modelsFolder) {
      setDisplayFolder(modelsFolder);
    } else if (defaultFolder) {
      setDisplayFolder(defaultFolder);
    }
  }, [modelsFolder, defaultFolder]);

  const handleSelectFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Models Folder',
      });
      if (selected && typeof selected === 'string') {
        setModelsFolder(selected);
        setDisplayFolder(selected);
      }
    } catch (error) {
      console.error('Failed to open folder picker:', error);
    }
  };

  const handleResetFolder = () => {
    setModelsFolder(null);
    setDisplayFolder(defaultFolder);
  };

  const steps = [
    {
      number: 1,
      type: 'transcription',
      title: 'Download Transcription Engine',
    },
    {
      number: 2,
      type: 'summarization',
      title: 'Download Summarization Engine',
    },
  ];

  const handleContinue = async () => {
    // Save the models folder before proceeding to download step
    if (modelsFolder) {
      try {
        const { configService } = await import('@/services/configService');
        await configService.setModelsFolder(modelsFolder);
        console.log('[SetupOverview] Models folder saved before downloads:', modelsFolder);
      } catch (e) {
        console.error('[SetupOverview] Failed to save models folder:', e);
      }
    }
    goNext();
  };

  const isCustomFolder = modelsFolder !== null && modelsFolder !== defaultFolder;

  return (
    <OnboardingContainer
      title="Setup Overview"
      description="Meetily requires that you download the Transcription & Summarization AI models for the software to work."
      step={2}
      totalSteps={isMac ? 4 : 3}
    >
      <div className="flex flex-col items-center space-y-10">
        {/* Steps Card */}
        <div className="w-full max-w-md bg-white rounded-lg border border-gray-200 p-4">
          <div className="space-y-4">
            {steps.map((step, idx) => {
              return (
                <div
                  key={step.number}
                  className={`flex items-start gap-4 p-1`}
                >
                  <div className="flex-1 ml-1">
                    <h3 className="font-medium text-gray-900 flex items-center gap-2">
                        Step {step.number} :  {step.title}

                        {step.type === "summarization" && (
                            <TooltipProvider>
                            <Tooltip>
                                <TooltipTrigger asChild>
                                <button className="text-gray-400 hover:text-gray-600">
                                    <Info className="w-4 h-4" />
                                </button>
                                </TooltipTrigger>
                                <TooltipContent className="max-w-xs text-sm">
                                You can also select external AI providers like OpenAI, Claude, or
                                Ollama for summary generation in settings.
                                </TooltipContent>
                            </Tooltip>
                            </TooltipProvider>
                        )}
                        </h3>
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Models Folder Selection */}
        <div className="w-full max-w-md bg-white rounded-lg border border-gray-200 p-4">
          <div className="flex items-start gap-3">
            <Folder className="w-5 h-5 text-gray-400 mt-0.5 flex-shrink-0" />
            <div className="flex-1 min-w-0">
              <h3 className="font-medium text-gray-900 text-sm">Models Storage Location</h3>
              <p className="text-xs text-gray-500 mt-1 mb-3">
                Choose where to store AI models. A folder with ~2-3 GB free space is recommended.
              </p>

              <div className="bg-gray-50 rounded-md p-2.5 mb-2">
                <p className="text-xs text-gray-700 break-all font-mono">
                  {displayFolder || 'Loading...'}
                </p>
                {isCustomFolder && (
                  <p className="text-xs text-amber-600 mt-1">
                    Custom folder selected
                  </p>
                )}
              </div>

              <div className="flex gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={handleSelectFolder}
                  className="text-xs h-8"
                >
                  <Folder className="w-3.5 h-3.5 mr-1.5" />
                  Change Folder
                </Button>
                {isCustomFolder && (
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={handleResetFolder}
                    className="text-xs h-8 text-gray-500"
                  >
                    <RotateCcw className="w-3.5 h-3.5 mr-1.5" />
                    Reset to Default
                  </Button>
                )}
              </div>
            </div>
          </div>
        </div>

        {/* CTA Section */}
        <div className="w-full max-w-xs space-y-4">
          <Button
            onClick={handleContinue}
            className="w-full h-11 bg-gray-900 hover:bg-gray-800 text-white"
          >
            Let's Go
          </Button>
          <div className="text-center">
            <a
              href="https://github.com/Zackriya-Solutions/meeting-minutes"
              target="_blank"
              rel="noopener noreferrer"
              className="text-xs text-gray-600 hover:underline"
            >
              Report issues on GitHub
            </a>
          </div>
        </div>
      </div>
    </OnboardingContainer>
  );
}
