"""Build the student import template served at /templates/ncrm-students.xlsx.

Run from the repo root: python3 scripts/make_student_template.py
The column titles must match what crates/ncrm-core/src/import.rs reads.
"""
import csv
from datetime import date

from openpyxl import Workbook
from openpyxl.styles import Font, PatternFill
from openpyxl.worksheet.datavalidation import DataValidation

COLUMNS = [
    ("Name", 26, "Aarav Sharma"),
    ("Phone", 18, "98765 43210"),
    ("Contact name", 22, "Neha Sharma"),
    ("Relation", 14, "Mother"),
    ("Date of birth", 15, date(2015, 3, 4)),
    ("Joined on", 15, date(2024, 4, 3)),
    ("Branch", 18, "Hauz Khas"),
    # Left empty in the example: a class name only works once that class
    # exists in the studio, and the example row should import cleanly.
    ("Class", 26, None),
    ("Monthly fee", 14, None),
    ("Notes", 34, "Shy at first; loves Bollywood"),
]

book = Workbook()
ws = book.active
ws.title = "Students"
header_font = Font(bold=True)
fill = PatternFill("solid", fgColor="E8F3EE")
for i, (title, width, example) in enumerate(COLUMNS, start=1):
    cell = ws.cell(row=1, column=i, value=title)
    cell.font = header_font
    cell.fill = fill
    ws.column_dimensions[cell.column_letter].width = width
    ex = ws.cell(row=2, column=i, value=example)
    if isinstance(example, date):
        ex.number_format = "dd/mm/yyyy"
# Phones as text, so Excel doesn't turn them into 9.88E+09.
for row in range(2, 2002):
    ws.cell(row=row, column=2).number_format = "@"
    for col in (5, 6):
        ws.cell(row=row, column=col).number_format = "dd/mm/yyyy"
relation = DataValidation(type="list", formula1='"Mother,Father,Guardian,Self,Other"', allow_blank=True)
ws.add_data_validation(relation)
relation.add("D2:D2001")
ws.freeze_panes = "A2"

help_ws = book.create_sheet("How to fill this in")
lines = [
    "One student per row. Only Name is required.",
    "Phone: the WhatsApp number to message about this student. For a child, a parent's number.",
    "Contact name and Relation: whose number it is. Leave them empty for an adult's own number.",
    "Dates as DD/MM/YYYY. Joined on can be in the past; empty means the day you import.",
    "Branch must match a branch name in ncrm; otherwise the branch you choose when importing is used.",
    "Class: the class's name as it is in ncrm, to put the student in it from today. Several classes: separate them with commas. Empty: add them to classes later.",
    "Monthly fee: for students with a Class, the name of a monthly price in ncrm or its amount (2500). Empty: they pay with class credits until you choose.",
    "You can also use your own sheet: columns like Student name, Mobile, Mother's phone, Father's phone, DOB and Date of joining are recognised.",
    "Delete the example row before importing, or ncrm will add Aarav Sharma too.",
]
for i, line in enumerate(lines, start=1):
    help_ws.cell(row=i, column=1, value=line)
help_ws.column_dimensions["A"].width = 110
book.save("web/public/templates/ncrm-students.xlsx")

with open("web/public/templates/ncrm-students.csv", "w", newline="", encoding="utf-8-sig") as f:
    w = csv.writer(f)
    w.writerow([c[0] for c in COLUMNS])
    w.writerow([
        c[2].strftime("%d/%m/%Y") if isinstance(c[2], date) else (c[2] or "") for c in COLUMNS
    ])
print("templates written")
