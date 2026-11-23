import type { Metadata } from "next";
import Link from "next/link";

import { NotAllowed } from "@/components/not-allowed";
import { getMe, getReceipt } from "@/lib/api-server";
import { formatDate, formatRupees } from "@/lib/format";
import { PAYMENT_METHOD_LABEL } from "@/lib/types";

import { PrintButton } from "./print-button";

export const metadata: Metadata = { title: "Receipt" };

export default async function ReceiptPage({ params }: PageProps<"/fees/receipts/[id]">) {
  const me = await getMe();
  if (!me.studio!.capabilities.includes("manage_fees")) return <NotAllowed what="receipts" />;
  const { id } = await params;
  const { payment, studio_name, lines, still_due_paise } = await getReceipt(id);
  const cancelled = payment.voided_at !== null;

  return (
    <div className="mx-auto flex max-w-[560px] flex-col gap-[18px]">
      <div className="no-print flex flex-wrap items-center justify-between gap-3">
        <Link href={`/students/${payment.student_id}#fees`} className="btn btn-quiet px-0">
          {payment.student_name}
        </Link>
        <PrintButton />
      </div>

      <article aria-labelledby="receipt-title" className="receipt flex flex-col gap-5 p-7">
        <header className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <p className="font-display m-0 text-[22px] leading-tight font-semibold">{studio_name}</p>
            <h1 id="receipt-title" className="m-0 mt-1 text-sm font-semibold">
              Receipt {payment.receipt_no}
            </h1>
          </div>
          <p className="m-0 text-right text-sm">
            {formatDate(payment.received_on)}
            {cancelled ? <span className="tag tag-attn mt-1 block">Cancelled</span> : null}
          </p>
        </header>

        <div>
          <p className="muted m-0 text-[13px]">Received from</p>
          <p className="m-0 text-lg font-bold">{payment.student_name}</p>
        </div>

        <div>
          <p className={`font-display m-0 text-[44px] leading-none font-semibold tabular-nums ${cancelled ? "line-through" : ""}`}>
            {formatRupees(payment.amount_paise)}
          </p>
          <p className="soft m-0 mt-2 text-sm">
            {PAYMENT_METHOD_LABEL[payment.method]}
            {payment.reference ? `, ${payment.reference}` : ""}
          </p>
        </div>

        {cancelled ? (
          <p className="attn-text m-0 font-semibold">This payment was cancelled: {payment.void_reason}</p>
        ) : (
          <table className="w-full border-collapse text-sm">
            <caption className="sr-only">What this payment was for</caption>
            <thead>
              <tr>
                <th scope="col" className="muted pb-2 text-left text-[13px] font-medium">
                  For
                </th>
                <th scope="col" className="muted pb-2 text-right text-[13px] font-medium">
                  Amount
                </th>
              </tr>
            </thead>
            <tbody>
              {lines.map((l, i) => (
                <tr key={`${l.description}-${i}`} className="receipt-line">
                  <td className="py-2 pr-3 font-semibold">{l.description}</td>
                  <td className="py-2 text-right font-semibold tabular-nums">{formatRupees(l.amount_paise)}</td>
                </tr>
              ))}
              {payment.unallocated_paise > 0 ? (
                <tr className="receipt-line">
                  <td className="py-2 pr-3 font-semibold">Paid in advance</td>
                  <td className="py-2 text-right font-semibold tabular-nums">{formatRupees(payment.unallocated_paise)}</td>
                </tr>
              ) : null}
            </tbody>
          </table>
        )}

        <footer className="flex flex-wrap items-end justify-between gap-3 text-sm">
          <p className="m-0">
            <span className="muted block text-[13px]">Still to pay</span>
            <span className="font-bold tabular-nums">{still_due_paise > 0 ? formatRupees(still_due_paise) : "Nothing"}</span>
          </p>
          <p className="m-0 text-right">
            <span className="muted block text-[13px]">Received by</span>
            <span className="font-semibold">{payment.received_by_name}</span>
          </p>
        </footer>
        {payment.note ? <p className="soft m-0 text-sm">Note: {payment.note}</p> : null}
      </article>
    </div>
  );
}
