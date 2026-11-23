/** Centered single-column layout for signing in and picking a studio. */
export default function AuthLayout({ children }: LayoutProps<"/">) {
  return (
    <main className="mx-auto flex min-h-dvh w-full max-w-[460px] flex-col justify-center gap-6 px-[18px] py-10">
      <div className="font-display text-[26px] font-semibold">ncrm</div>
      {children}
    </main>
  );
}
