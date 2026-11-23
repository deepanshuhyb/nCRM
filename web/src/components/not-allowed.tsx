/** Shown when someone opens a page their role can't use. */
export function NotAllowed({ what }: { what: string }) {
  return (
    <section className="panel mx-auto max-w-[560px] p-7">
      <h1 className="h-section">You can&apos;t open {what}.</h1>
      <p className="soft mt-2 mb-0">Your role doesn&apos;t include this. Ask the studio owner if you need it.</p>
    </section>
  );
}
