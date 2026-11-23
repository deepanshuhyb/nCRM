"use client";

type Cell = string | number;

/**
 * One cell, safe for a spreadsheet. Quotes are doubled and the cell is
 * wrapped in quotes. A cell that starts with =, +, - or @ would be run as
 * a formula by Excel, and a student's name is text someone else typed,
 * so those get a leading apostrophe, which spreadsheets show as plain text.
 */
function cell(value: Cell): string {
  if (typeof value === "number") return String(value);
  const text = /^[=+\-@\t\r]/.test(value) ? `'${value}` : value;
  return `"${text.replace(/"/g, '""')}"`;
}

/**
 * A button that saves a table as a .csv file. The file is built in the
 * browser from numbers the page already has: nothing is requested, and
 * no third party is involved.
 */
export function DownloadCsv({ filename, headers, rows, label = "Download" }: { filename: string; headers: string[]; rows: Cell[][]; label?: string }) {
  function download() {
    const lines = [headers, ...rows].map((r) => r.map(cell).join(","));
    // The first three bytes tell Excel the file is UTF-8, so ₹ and names
    // in other scripts come out right.
    const blob = new Blob(["﻿", lines.join("\r\n"), "\r\n"], { type: "text/csv;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  return (
    <button type="button" className="btn btn-quiet min-h-[40px] px-2 text-sm" onClick={download} disabled={rows.length === 0}>
      {label}
    </button>
  );
}
