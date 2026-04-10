export default function Page() {
  return (
    <main className="page">
      <div className="card">
        <p className="eyebrow">Deku Template</p>
        <h1>Next.js starter</h1>
        <p>
          This is a plain App Router starter shaped for Deku source deploys.
        </p>
        <ul>
          <li>Builder: Dockerfile</li>
          <li>Runtime: Next.js standalone</li>
          <li>Health endpoint: /health</li>
        </ul>
      </div>
    </main>
  );
}
