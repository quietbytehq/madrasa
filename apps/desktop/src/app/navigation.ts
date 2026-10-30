import {
  BarChart3,
  CalendarCheck,
  Coins,
  DatabaseBackup,
  FileSpreadsheet,
  GraduationCap,
  LayoutDashboard,
  Network,
  School,
  Settings,
  UserRound,
  Users,
  UsersRound,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { Feature } from "@/api/types";
import type { TranslationKey } from "@/i18n";

export interface NavItem {
  path: string;
  labelKey: TranslationKey;
  icon: LucideIcon;
  feature: Feature;
  /** True once the module is actually built; otherwise the route shows "Coming soon". */
  available: boolean;
}

/** Sidebar modules in display order. Items whose feature is off for the edition are hidden. */
export const NAV_ITEMS: readonly NavItem[] = [
  {
    path: "/",
    labelKey: "nav.dashboard",
    icon: LayoutDashboard,
    feature: "dashboard",
    available: true,
  },
  {
    path: "/students",
    labelKey: "nav.students",
    icon: Users,
    feature: "students",
    available: true,
  },
  {
    path: "/import",
    labelKey: "nav.import",
    icon: FileSpreadsheet,
    feature: "students",
    available: true,
  },
  {
    path: "/attendance",
    labelKey: "nav.attendance",
    icon: CalendarCheck,
    feature: "attendance",
    available: true,
  },
  {
    path: "/exams",
    labelKey: "nav.examinations",
    icon: GraduationCap,
    feature: "examinations",
    available: true,
  },
  { path: "/fees", labelKey: "nav.fees", icon: Coins, feature: "fees", available: true },
  {
    path: "/teachers",
    labelKey: "nav.teachers",
    icon: UserRound,
    feature: "teachers",
    available: true,
  },
  {
    path: "/reports",
    labelKey: "nav.reports",
    icon: BarChart3,
    feature: "reports",
    available: false,
  },
  {
    path: "/backup",
    labelKey: "nav.backup",
    icon: DatabaseBackup,
    feature: "backup",
    available: true,
  },
  {
    path: "/settings",
    labelKey: "nav.settings",
    icon: Settings,
    feature: "settings",
    available: true,
  },
  {
    path: "/campuses",
    labelKey: "nav.campuses",
    icon: School,
    feature: "campuses",
    available: false,
  },
  {
    path: "/parents",
    labelKey: "nav.parents",
    icon: UsersRound,
    feature: "parents_portal",
    available: false,
  },
  {
    path: "/network",
    labelKey: "nav.network",
    icon: Network,
    feature: "nexus_hub",
    available: false,
  },
];

export function visibleNavItems(has: (feature: Feature) => boolean): NavItem[] {
  return NAV_ITEMS.filter((item) => has(item.feature));
}
