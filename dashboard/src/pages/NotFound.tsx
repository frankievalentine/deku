import { Link } from '@tanstack/react-router';

export function NotFound() {
  return (
    <div className="mx-auto max-w-[1280px] py-16">
      <h1 className="font-display text-[26px]">Nothing here</h1>
      <p className="mt-2 text-ink-2">
        That page does not exist.{' '}
        <Link to="/" className="underline">
          Back to apps
        </Link>
      </p>
    </div>
  );
}
