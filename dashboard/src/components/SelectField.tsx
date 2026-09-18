// biome-ignore-all lint/a11y/useFocusableInteractive: Basecoat's listbox is a composite widget; focus stays on the trigger and the active option is conveyed with aria-activedescendant, so role="option" elements are intentionally not focusable.
import { useEffect, useRef, useState } from 'react';

export interface SelectFieldHandle {
  focus: () => void;
}

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

interface SelectFieldProps {
  /** Applied to the trigger button; the visible `<label>` points at it. */
  id: string;
  value: string;
  options: SelectOption[];
  onChange: (value: string) => void;
  /** Accessible name when no visible `<label>` is rendered. */
  label?: string;
  placeholder?: string;
  /** Shown in the listbox when there are no options; Basecoat's own default reads "No_results_found". */
  emptyMessage?: string;
  disabled?: boolean;
  /** Marks the trigger invalid and points assistive tech at the described error. */
  invalid?: boolean;
  describedBy?: string;
  /** Lets a caller move focus to the trigger, e.g. after a validation failure. */
  handleRef?: React.RefObject<SelectFieldHandle | null>;
}

interface BasecoatSelectElement extends HTMLDivElement {
  refresh?: () => void;
  open?: () => void;
  close?: (focusOnTrigger?: boolean) => void;
}

/**
 * Basecoat's `div.select`, wrapped for React.
 *
 * The native control is replaced because the browser draws its option list,
 * which cannot be styled or width-constrained. The markup follows
 * https://basecoatui.com/components/select/ — the trigger button carries the
 * width, the root carries `data-placeholder`, and the popover inherits the
 * trigger's width because it is anchored to the root.
 *
 * Basecoat reads the option list once at init, and React owns the children, so
 * `refresh()` is called after every render, which is the documented way to
 * rescan options. Selection is lifted back out through Basecoat's own `change`
 * event.
 */
export default function SelectField({
  id,
  value,
  options,
  onChange,
  label,
  placeholder,
  emptyMessage = 'No options available',
  disabled = false,
  invalid = false,
  describedBy,
  handleRef,
}: SelectFieldProps) {
  const rootRef = useRef<BasecoatSelectElement | null>(null);
  const onChangeRef = useRef(onChange);
  const [open, setOpen] = useState(false);

  onChangeRef.current = onChange;

  useEffect(() => {
    if (!handleRef) return;
    handleRef.current = {
      focus: () => rootRef.current?.querySelector<HTMLButtonElement>(':scope > button')?.focus(),
    };
    return () => {
      handleRef.current = null;
    };
  }, [handleRef]);

  const selected = options.find((option) => option.value === value);
  const listboxId = `${id}-listbox`;
  const popoverId = `${id}-popover`;

  useEffect(() => {
    rootRef.current?.refresh?.();
  });

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;

    const trigger = root.querySelector<HTMLButtonElement>(':scope > button');
    if (!trigger) return;

    const syncExpanded = () => setOpen(trigger.getAttribute('aria-expanded') === 'true');
    const observer = new MutationObserver(syncExpanded);
    observer.observe(trigger, { attributes: true, attributeFilter: ['aria-expanded'] });

    const handleChange = (event: Event) => {
      const detail = (event as CustomEvent<{ value?: string }>).detail;
      if (detail && typeof detail.value === 'string') {
        onChangeRef.current(detail.value);
      }
    };

    // Basecoat closes on document clicks but not when the window itself loses
    // focus, so a select opened and then left open while another window is used
    // would stay expanded. Close it on blur without stealing focus back.
    const closeOnBlur = () => root.close?.(false);
    window.addEventListener('blur', closeOnBlur);

    root.addEventListener('change', handleChange);
    return () => {
      observer.disconnect();
      root.removeEventListener('change', handleChange);
      window.removeEventListener('blur', closeOnBlur);
    };
  }, []);

  function handleTriggerKeyDown(event: React.KeyboardEvent<HTMLButtonElement>) {
    // Basecoat opens on Arrow keys, Home/End and Enter, but not on Space.
    if (event.key === ' ' || event.key === 'Spacebar') {
      event.preventDefault();
      rootRef.current?.open?.();
    }
  }

  return (
    <div
      id={`${id}-select`}
      className="select"
      data-placeholder={placeholder}
      ref={rootRef}
      data-value={value}
    >
      <button
        type="button"
        id={id}
        className="input select-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={listboxId}
        aria-label={label}
        aria-invalid={invalid || undefined}
        aria-describedby={describedBy}
        disabled={disabled}
        onKeyDown={handleTriggerKeyDown}
      >
        <span className="truncate">{selected?.label ?? placeholder ?? ''}</span>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="24"
          height="24"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="m6 9 6 6 6-6" />
        </svg>
      </button>
      <div id={popoverId} data-popover aria-hidden="true">
        <div
          role="listbox"
          id={listboxId}
          aria-orientation="vertical"
          aria-labelledby={id}
          data-empty={emptyMessage}
        >
          {options.map((option) => (
            <div
              key={option.value}
              role="option"
              data-value={option.value}
              aria-selected={option.value === value || undefined}
              aria-disabled={option.disabled || undefined}
            >
              {option.label}
            </div>
          ))}
        </div>
      </div>
      <input type="hidden" value={value} readOnly tabIndex={-1} aria-hidden="true" />
    </div>
  );
}
