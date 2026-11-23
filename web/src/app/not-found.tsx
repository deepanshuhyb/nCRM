import Link from "next/link";

export default function NotFound() {
  return (
    <main className="mx-auto flex min-h-dvh max-w-[520px] flex-col justify-center px-[18px]">
      <section className="panel p-7">
        <h1 className="h-section">That page doesn&apos;t exist.</h1>
        <p className="soft mt-2 mb-5">It may have moved, or the link is wrong.</p>
        <Link href="/" className="btn btn-primary">
          Go to home
        </Link>
      </section>
    </main>
  );
}
