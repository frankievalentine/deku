import { useCallback, useEffect, useId, useRef } from 'react';

interface ConfirmModalProps {
  open: boolean;
  title: string;
  description: string;
  confirmLabel?: string;
  cancelLabel?: string;
  busy?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}

export default function ConfirmModal({
  open,
  title,
  description,
  confirmLabel = 'Confirm',
  cancelLabel = 'Cancel',
  busy = false,
  onConfirm,
  onClose,
}: ConfirmModalProps) {
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const closeNotifiedRef = useRef(false);
  const titleId = useId();
  const descriptionId = useId();

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;

    if (open && !dialog.open) {
      closeNotifiedRef.current = false;
      dialog.showModal();
      return;
    }

    if (!open && dialog.open) {
      dialog.close();
    }
  }, [open]);

  const notifyClose = useCallback(() => {
    if (closeNotifiedRef.current) return;
    closeNotifiedRef.current = true;
    onClose();
  }, [onClose]);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog || !open) return;

    const dialogElement = dialog;
    const handleDialogClick = (event: MouseEvent) => {
      if (event.target !== dialogElement) return;

      const rect = dialogElement.getBoundingClientRect();
      const isInsideDialog =
        event.clientX >= rect.left &&
        event.clientX <= rect.right &&
        event.clientY >= rect.top &&
        event.clientY <= rect.bottom;

      if (!isInsideDialog && !busy) notifyClose();
    };

    dialogElement.addEventListener('click', handleDialogClick);
    return () => dialogElement.removeEventListener('click', handleDialogClick);
  }, [busy, notifyClose, open]);

  return (
    <dialog
      ref={dialogRef}
      className="modal-shell"
      aria-labelledby={titleId}
      aria-describedby={descriptionId}
      onCancel={(event) => {
        if (busy) event.preventDefault();
      }}
      onClose={notifyClose}
    >
      {open ? (
        <>
          <div className="modal-badge">Destructive action</div>
          <div className="stack-md">
            <h2 id={titleId} className="modal-title">
              {title}
            </h2>
            <p id={descriptionId} className="modal-copy">
              {description}
            </p>
          </div>
          <div className="form-actions">
            <button
              type="button"
              className="btn btn-secondary"
              onClick={notifyClose}
              disabled={busy}
            >
              {cancelLabel}
            </button>
            <button type="button" className="btn btn-danger" onClick={onConfirm} disabled={busy}>
              {busy ? 'Working…' : confirmLabel}
            </button>
          </div>
        </>
      ) : null}
    </dialog>
  );
}
