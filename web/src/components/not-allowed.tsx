/** Shown when someone opens a page their role can't use. */
export function NotAllowed({ what }: { what: string }) {
  return (
    <section className="glass mx-auto max-w-[560px] p-7">
      <h1 className="font-display m-0 text-[24px] font-semibold">You can&apos;t open {what}.</h1>
      <p className="soft mt-2 mb-0">Your role doesn&apos;t include this. Ask the studio owner if you need it.</p>
    </section>
  );
}
