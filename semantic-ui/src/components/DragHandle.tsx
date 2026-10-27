export function DragHandle() {
  return (
    <span className="drag-handle" aria-hidden="true">
      <svg viewBox="0 0 12 16" width="12" height="16" fill="currentColor" aria-hidden="true">
        <path d="M6 1 L10 5 H2 Z" />
        <rect x="1" y="6.2" width="10" height="1.5" rx="0.4" />
        <rect x="1" y="8.3" width="10" height="1.5" rx="0.4" />
        <path d="M6 15 L2 11 H10 Z" />
      </svg>
    </span>
  );
}
