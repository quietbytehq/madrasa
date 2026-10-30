import { useEffect, useState } from "react";
import {
  Bell,
  Check,
  CircleHelp,
  Languages,
  LogOut,
  Monitor,
  Moon,
  Search,
  Sun,
  UserRound,
} from "lucide-react";
import { useNavigate } from "react-router-dom";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { useI18n } from "@/i18n";
import { LOCALES } from "@/i18n";
import { useTheme } from "@/theme";
import type { ThemePreference } from "@/theme";
import { useSession } from "@/app/session";
import { useFeatures } from "@/app/features";
import { Menu, MenuItem, MenuSection } from "./Menu";

const iconButton =
  "flex h-8 w-8 items-center justify-center rounded text-muted hover:bg-hover hover:text-text aria-expanded:bg-hover";

export function TopBar() {
  const { t, locale, setLocale } = useI18n();
  const { preference, setPreference } = useTheme();
  const { user, logout } = useSession();
  const { has } = useFeatures();
  const navigate = useNavigate();
  const [searchOpen, setSearchOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);

  // Ctrl+K / Cmd+K opens global search.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setSearchOpen(true);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  const themes: Array<{ value: ThemePreference; icon: typeof Sun; label: string }> = [
    { value: "system", icon: Monitor, label: t("topbar.theme.system") },
    { value: "light", icon: Sun, label: t("topbar.theme.light") },
    { value: "dark", icon: Moon, label: t("topbar.theme.dark") },
  ];

  return (
    <header className="flex h-topbar shrink-0 items-center gap-2 border-b border-border bg-card px-3">
      <button
        type="button"
        onClick={() => setSearchOpen(true)}
        className="flex h-8 w-72 max-w-full items-center gap-2 rounded border border-border bg-bg px-2.5 text-start text-sm text-muted hover:bg-hover"
      >
        <Search className="h-4 w-4" aria-hidden />
        <span className="flex-1 truncate">{t("topbar.search")}</span>
      </button>

      {has("campuses") && (
        <div className="ms-2 hidden items-center gap-1 text-xs text-muted md:flex">
          <span>{t("topbar.campus")}:</span>
          <span className="font-medium text-text">—</span>
        </div>
      )}

      <div className="flex-1" />

      <Menu
        label={t("topbar.notifications")}
        trigger={({ open, toggle, id }) => (
          <button
            type="button"
            onClick={toggle}
            aria-label={t("topbar.notifications")}
            aria-expanded={open}
            aria-controls={id}
            className={iconButton}
          >
            <Bell className="h-4 w-4" aria-hidden />
          </button>
        )}
      >
        {() => (
          <EmptyState
            className="py-6"
            title={t("topbar.notifications")}
            description={t("topbar.notificationsEmpty")}
          />
        )}
      </Menu>

      <button
        type="button"
        onClick={() => setHelpOpen(true)}
        aria-label={t("topbar.help")}
        className={iconButton}
      >
        <CircleHelp className="h-4 w-4" aria-hidden />
      </button>

      <Menu
        label={t("topbar.userMenu")}
        trigger={({ open, toggle, id }) => (
          <button
            type="button"
            onClick={toggle}
            aria-label={t("topbar.userMenu")}
            aria-expanded={open}
            aria-controls={id}
            className="flex h-8 items-center gap-2 rounded px-1.5 hover:bg-hover aria-expanded:bg-hover"
          >
            <span className="flex h-6 w-6 items-center justify-center rounded-full bg-lavender text-navy">
              <UserRound className="h-3.5 w-3.5" aria-hidden />
            </span>
            <span className="hidden max-w-32 truncate text-sm font-medium text-text md:inline">
              {user?.full_name}
            </span>
          </button>
        )}
      >
        {(close) => (
          <>
            <div className="px-2 py-1.5">
              <div className="truncate text-sm font-medium text-text">{user?.full_name}</div>
              <div className="truncate text-xs text-muted">{user?.username}</div>
            </div>
            <MenuSection title={t("topbar.language")}>
              {LOCALES.map((l) => (
                <MenuItem
                  key={l}
                  active={locale === l}
                  icon={
                    locale === l ? (
                      <Check className="h-4 w-4" aria-hidden />
                    ) : (
                      <Languages className="h-4 w-4" aria-hidden />
                    )
                  }
                  onClick={() => setLocale(l)}
                >
                  {t(`lang.${l}`)}
                </MenuItem>
              ))}
            </MenuSection>
            <MenuSection title={t("topbar.theme")}>
              {themes.map(({ value, icon: Icon, label }) => (
                <MenuItem
                  key={value}
                  active={preference === value}
                  icon={<Icon className="h-4 w-4" aria-hidden />}
                  onClick={() => setPreference(value)}
                >
                  {label}
                </MenuItem>
              ))}
            </MenuSection>
            <MenuSection>
              <MenuItem
                icon={<LogOut className="h-4 w-4" aria-hidden />}
                onClick={() => {
                  close();
                  void logout().then(() => navigate("/login"));
                }}
              >
                {t("action.logout")}
              </MenuItem>
            </MenuSection>
          </>
        )}
      </Menu>

      <Dialog
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        title={t("topbar.searchTitle")}
        size="lg"
      >
        <input
          type="search"
          autoFocus
          placeholder={t("action.search")}
          aria-label={t("topbar.searchTitle")}
          className="h-9 w-full rounded border border-border bg-bg px-3 text-sm text-text placeholder:text-muted"
        />
        <p className="mt-3 text-xs text-muted">{t("topbar.searchHint")}</p>
      </Dialog>

      <Dialog open={helpOpen} onClose={() => setHelpOpen(false)} title={t("topbar.help")} size="sm">
        <p className="text-sm text-text">{t("topbar.helpBody")}</p>
      </Dialog>
    </header>
  );
}
