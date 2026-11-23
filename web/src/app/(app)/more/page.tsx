import type { Metadata } from "next";
import Link from "next/link";

import { getMe } from "@/lib/api-server";
import { BOTTOM_NAV_SLOTS, navFor } from "@/lib/nav";

export const metadata: Metadata = { title: "More" };

/** The pages that don't fit in the phone's bottom bar. */
export default async function MorePage() {
  const me = await getMe();
  const rest = navFor(me.studio!.capabilities).slice(BOTTOM_NAV_SLOTS);
  return (
    <div className="mx-auto flex max-w-[560px] flex-col gap-[18px]">
      <h1 className="font-display m-0 text-[32px] leading-tight font-semibold">More</h1>
      <ul className="glass m-0 flex list-none flex-col gap-1.5 p-2">
        {rest.map((item) => (
          <li key={item.href}>
            <Link href={item.href} className="row list-row min-h-[56px]">
              <span className="list-row-title">{item.label}</span>
            </Link>
          </li>
        ))}
      </ul>
    </div>
  );
}
