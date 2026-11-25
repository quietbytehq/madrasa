import type { OutboxStatus } from "@/api/types";
import { useConnection } from "@/app/connection";
import { useEffect, useMemo, useRef } from "react";

/**
 * Fee payments taken on this PC while the school server was out of reach, not yet sent.
 *
 * The voucher, the ledger and the student's balance come from the server (or from the copy
 * kept for offline reading), so they know nothing of such a payment until it arrives. Every
 * screen that shows a voucher adds this on top, so nobody collects the same money twice
 * (found when v0.5.0 was tested: after Rs 5,000 was taken offline the ledger still said
 * "unpaid, Rs 9,240").
 */
export interface WaitingPayment {
  /** Sum of the payments waiting for this voucher. */
  amount: number;
  /** Their provisional receipt numbers. */
  numbers: string[];
}

export function waitingPayments(outbox: OutboxStatus | null): Map<string, WaitingPayment> {
  const byVoucher = new Map<string, WaitingPayment>();
  for (const item of outbox?.items ?? []) {
    if (!item.voucher_id || !item.provisional_no) continue;
    const amount = Number(item.amount ?? 0);
    if (!Number.isFinite(amount) || amount <= 0) continue;
    const w = byVoucher.get(item.voucher_id) ?? { amount: 0, numbers: [] };
    w.amount += amount;
    w.numbers.push(item.provisional_no);
    byVoucher.set(item.voucher_id, w);
  }
  return byVoucher;
}

/** Waiting payments by voucher id, kept up to date with the connection badge. */
export function useWaitingPayments(): Map<string, WaitingPayment> {
  const outbox = useConnection()?.outbox ?? null;
  return useMemo(() => waitingPayments(outbox), [outbox]);
}

/** What is left of `balance` once the waiting payments have arrived (never below zero). */
export function balanceAfterWaiting(balance: string | number, waiting: number): string {
  return Math.max(Number(balance) - waiting, 0).toFixed(2);
}

/**
 * Read the screen's figures again the moment a waiting payment has reached the server.
 *
 * While the payment waits, the note beside the voucher covers for the old figures. When it
 * arrives the note goes; without this the screen would go on showing "unpaid" from before,
 * with nothing to say otherwise, until someone opened it again.
 */
export function useReloadWhenPaymentsArrive(reload: () => unknown): void {
  const waiting = useWaitingPayments();
  const numbers = [...waiting.values()]
    .flatMap((w) => w.numbers)
    .sort()
    .join(",");
  const before = useRef(numbers);
  const reloadRef = useRef(reload);
  reloadRef.current = reload;
  useEffect(() => {
    const was = before.current ? before.current.split(",") : [];
    const now = new Set(numbers ? numbers.split(",") : []);
    before.current = numbers;
    if (was.some((n) => !now.has(n))) void reloadRef.current();
  }, [numbers]);
}
