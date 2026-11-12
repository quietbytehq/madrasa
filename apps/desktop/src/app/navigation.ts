import {
  BarChart3,
  BedDouble,
  BookOpen,
  Bus,
  CalendarCheck,
  CalendarClock,
  CalendarRange,
  ClipboardList,
  Coins,
  DatabaseBackup,
  FileSpreadsheet,
  GraduationCap,
  IdCard,
  LayoutDashboard,
  MessagesSquare,
  Network,
  Package,
  School,
  Settings,
  UserRound,
  Users,
  UsersRound,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { Feature } from "@/api/types";
import type { TranslationKey } from "@/i18n";
import { PARENT_APP_PERMISSIONS } from "./parentAppPermissions";

export interface NavItem {
  path: string;
  labelKey: TranslationKey;
  icon: LucideIcon;
  feature: Feature;
  /** True once the module is actually built; otherwise the route shows "Coming soon". */
  available: boolean;
  /** Permission needed to use the module; items the user lacks are hidden from the sidebar. */
  permission?: string;
  /** The item is for anyone who holds at least one of these permissions. */
  anyPermission?: readonly string[];
  /** Shown only while the school is joined to a school network (a hub). */
  needsNetwork?: boolean;
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
    available: true,
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
  // Standard modules (docs/TASKS-STANDARD.md), hidden on Lite; "Coming soon" until built.
  {
    path: "/timetable",
    labelKey: "nav.timetable",
    icon: CalendarClock,
    feature: "timetable",
    available: true,
    permission: "timetable.read",
  },
  {
    path: "/leave",
    labelKey: "nav.leave",
    icon: CalendarRange,
    feature: "leave",
    available: true,
    permission: "leave.read",
  },
  {
    path: "/admissions",
    labelKey: "nav.admissions",
    icon: ClipboardList,
    feature: "admissions",
    available: true,
    permission: "admissions.read",
  },
  {
    path: "/library",
    labelKey: "nav.library",
    icon: BookOpen,
    feature: "library",
    available: true,
    permission: "library.read",
  },
  {
    path: "/transport",
    labelKey: "nav.transport",
    icon: Bus,
    feature: "transport",
    available: true,
    permission: "transport.read",
  },
  {
    path: "/hostel",
    labelKey: "nav.hostel",
    icon: BedDouble,
    feature: "hostel",
    available: true,
    permission: "hostel.read",
  },
  {
    path: "/inventory",
    labelKey: "nav.inventory",
    icon: Package,
    feature: "inventory",
    available: true,
    permission: "inventory.read",
  },
  {
    path: "/hr",
    labelKey: "nav.hr",
    icon: IdCard,
    feature: "hr",
    available: false,
  },
  {
    path: "/campuses",
    labelKey: "nav.campuses",
    icon: School,
    feature: "campuses",
    available: false,
  },
  {
    path: "/parent-app",
    labelKey: "nav.parentApp",
    icon: MessagesSquare,
    feature: "nexus_link",
    available: true,
    anyPermission: PARENT_APP_PERMISSIONS,
    needsNetwork: true,
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
 * The Parent app entry also needs the school to be joined to a school network.
 */
export function visibleNavItems(
  has: (feature: Feature) => boolean,
  can: (permission: string) => boolean = () => true,
  /** The school is joined to a school network. */
  joined = false,
): NavItem[] {
  return NAV_ITEMS.filter(
    (item) =>
      has(item.feature) &&
      (!item.permission || can(item.permission)) &&
      (!item.anyPermission || item.anyPermission.some(can)) &&
      (!item.needsNetwork || joined),
  );
}
