"use client";

/** Opens the browser's print dialog; "Save as PDF" there makes a file to send. */
export function PrintButton() {
  return (
    <button type="button" className="btn btn-dark" onClick={() => window.print()}>
      Print or save as PDF
    </button>
  );
}
