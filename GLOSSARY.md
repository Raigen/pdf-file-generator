# PDF Invoice Generation

Turns structured invoice data into a print-ready PDF document, quickly, even when an invoice runs to hundreds of pages. It renders what it is given and never calculates amounts.

## Inputs

**Layout**:
The reusable visual design of an Invoice: page size, margins, Background, fonts, field positions and Line Item Table columns.
_Avoid_: Template, metadata, config

**Invoice Data**:
The per-run content of one Invoice: header values, Line Items and totals, all pre-computed and pre-formatted by the caller.
_Avoid_: Metadata, payload, records

## Parties

**Platform**:
The trusted, already-authenticated upstream system that requests Invoices from this service on behalf of Tenants.
_Avoid_: Issuer, client, caller

**Tenant**:
The business on whose behalf an Invoice is generated; it is the seller billing the Recipient and owns the Layout used.
_Avoid_: Client, customer, issuer

**Recipient**:
The party an Invoice is addressed to and who is asked to pay it.
_Avoid_: Customer, buyer, addressee

**Field**:
A place in the Layout where one value from the Invoice Data is drawn, at a fixed position and in a given style.
_Avoid_: Placeholder, slot, variable

**Static Text**:
Fixed wording defined by the Layout itself, such as labels, that does not come from the Invoice Data.
_Avoid_: Label, caption

## Document

**Invoice**:
The single archivable PDF document produced by one generation run, made up of one or more pages and optionally carrying an E-Invoice XML.
_Avoid_: Report, file, output

**Line Item**:
One entry in an invoice's list of charged goods or services; it occupies one row of the Line Item Table and is never split across pages.
_Avoid_: Record, row, entry, position

**Line Item Table**:
The borderless grid of columns in which all Line Items of an Invoice are laid out, continuing across as many pages as needed.
_Avoid_: List, grid

**Table Header**:
The row of column titles repeated at the top of the Line Item Table on every page.

**Background**:
The single raster image (PNG or JPEG) placed behind the content of every page of an Invoice, referenced by the Layout from file storage.
_Avoid_: Template, letterhead, watermark

**GiroCode**:
The EPC QR code on an Invoice that lets the Recipient start a SEPA transfer by scanning it.
_Avoid_: QR code, payment code

**E-Invoice XML**:
The machine-readable invoice (CII, per a declared ZUGFeRD profile) supplied by the Platform and embedded in the Invoice to make it a legally valid e-invoice.
_Avoid_: XRechnung, attachment, XML

## Page zones

**First-Page Header**:
The fixed-position block on page one carrying sender, recipient, invoice number and date.

**Continuation Header**:
The compact header on every page after the first, identifying the Invoice being continued.

**Totals Block**:
The net, tax and gross amounts, shown once directly after the last Line Item.
_Avoid_: Summary, footer

**Footer**:
The fixed block at the bottom of every page, including the page number in the form "page X of Y".
