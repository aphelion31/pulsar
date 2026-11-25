import { Link } from "react-router";

export default function NotFound() {
  return (
    <main className="grid min-h-dvh place-items-center px-6 text-center">
      <div>
        <h1 className="text-lg font-medium">Page not found</h1>
        <Link to="/" className="mt-3 inline-block text-accent hover:underline">
          Back to conversations
        </Link>
      </div>
    </main>
  );
}
