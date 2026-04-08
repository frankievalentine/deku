import type { ThemeMode } from '../lib/theme';
import Icon from './Icon';

interface ThemeModeToggleProps {
  mode: ThemeMode;
  onChange: (mode: ThemeMode) => void;
  compact?: boolean;
}

const OPTIONS: Array<{
  mode: ThemeMode;
  label: string;
  icon: 'theme-system' | 'theme-light' | 'theme-dark';
}> = [
  { mode: 'system', label: 'System', icon: 'theme-system' },
  { mode: 'light', label: 'Light', icon: 'theme-light' },
  { mode: 'dark', label: 'Dark', icon: 'theme-dark' },
];

export default function ThemeModeToggle({ mode, onChange, compact = false }: ThemeModeToggleProps) {
  const buttonClass = compact ? 'btn-sm-icon-outline' : 'btn-outline';

  return (
    <div className={`theme-toggle${compact ? ' is-compact' : ''}`}>
      {OPTIONS.map((option) => (
        <button
          key={option.mode}
          type="button"
          className={buttonClass}
          aria-pressed={mode === option.mode}
          title={option.label}
          onClick={() => onChange(option.mode)}
        >
          <Icon name={option.icon} size={16} />
          {compact ? <span className="sr-only">{option.label}</span> : <span>{option.label}</span>}
        </button>
      ))}
    </div>
  );
}
