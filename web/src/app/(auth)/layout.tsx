import { Counts } from "@/components/ui";

/**
 * Centered single column for signing in and picking a studio. The one
 * place (with Home) where the studio's count sits behind the page.
 */
export default function AuthLayout({ children }: LayoutProps<"/">) {
  return (
    <>
      <Counts />
      <main className="mx-auto flex min-h-dvh w-full max-w-[460px] flex-col justify-center gap-6 px-[18px] py-10">
        <div className="brand text-[28px]">ncrm</div>
        {children}
      </main>
    </>
  );
}
