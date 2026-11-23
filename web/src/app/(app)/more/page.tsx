import type { Metadata } from "next";
import Link from "next/link";

import { Page } from "@/components/ui";
import { getMe } from "@/lib/api-server";
import { BOTTOM_NAV_SLOTS, navFor } from "@/lib/nav";

export const metadata: Metadata = { title: "More" };

/** The pages that don't fit in the phone's bottom bar. */
export default async function MorePage() {
  const me = await getMe();
  const rest = navFor(me.studio!.capabilities).slice(BOTTOM_NAV_SLOTS);
  return (
    <Page narrow>
      <h1 className="h-page">More</h1>
      <ul className="panel m-0 flex list-none flex-col gap-1.5 p-2">
        {rest.map((item) => (
          <li key={item.href}>
            <Link href={item.href} className="row list-row min-h-[56px]">
              <span className="list-row-title">{item.label}</span>
            </Link>
          </li>
        ))}
      </ul>
    </Page>
  );
}
