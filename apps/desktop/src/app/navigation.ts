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
  /** Permission needed to use the module; items the user lacks are hidden from the sidebar. */
  permission?: string;
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
    permission: "students.read",
  },
  {
    path: "/import",
    labelKey: "nav.import",
    icon: FileSpreadsheet,
    feature: "students",
    available: true,
    permission: "students.manage",
  },
  {
    path: "/attendance",
    labelKey: "nav.attendance",
    icon: CalendarCheck,
    feature: "attendance",
    available: true,
    permission: "attendance.read",
  },
  {
    path: "/exams",
    labelKey: "nav.examinations",
    icon: GraduationCap,
    feature: "examinations",
    available: true,
    permission: "exams.read",
  },
  {
    path: "/fees",
    labelKey: "nav.fees",
    icon: Coins,
    feature: "fees",
    available: true,
    permission: "fees.read",
  },
  {
    path: "/teachers",
    labelKey: "nav.teachers",
    icon: UserRound,
    feature: "teachers",
    available: true,
    permission: "teachers.read",
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
    permission: "backup.manage",
  },
  {
    path: "/settings",
    labelKey: "nav.settings",
    icon: Settings,
    feature: "settings",
    available: true,
    permission: "academics.manage",
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

/**
 * Sidebar items for this user: the edition must enable the feature and, where an item names
 * a permission, the role must hold it (teachers see no Fees, Settings, Backup or Import).
 */
export function visibleNavItems(
  has: (feature: Feature) => boolean,
  can: (permission: string) => boolean = () => true,
): NavItem[] {
  return NAV_ITEMS.filter(
    (item) => has(item.feature) && (!item.permission || can(item.permission)),
  );
}
