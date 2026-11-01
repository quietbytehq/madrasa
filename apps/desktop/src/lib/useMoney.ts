import { useCallback } from "react";
import { useSchool } from "@/app/session";
import { formatMoney } from "@/lib/money";

/** Format an amount with the school's currency symbol, e.g. "Rs 2,500.00" or "$ 40.00". */
export function withCurrency(symbol: string, value: string | number | null | undefined): string {
  const text = formatMoney(value);
  return text === "—" ? text : `${symbol} ${text}`;
}

/** `money(value)` → the amount with the school's currency symbol. */
export function useMoney(): (value: string | number | null | undefined) => string {
  const { currency_symbol } = useSchool();
  return useCallback((value) => withCurrency(currency_symbol, value), [currency_symbol]);
}
