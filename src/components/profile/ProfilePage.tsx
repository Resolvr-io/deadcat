import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import { useCallback, useEffect, useState } from "react";
import { useEscapeKey } from "../../hooks/useEscapeKey";
import { useLockScroll } from "../../hooks/useLockScroll";
import { useStore } from "../../store";
import { generateAvatarDataUri } from "../../utils-react/avatar";
import { CloseButton } from "../shared/CloseButton";
import { showToast } from "../shared/Toast";

const NOSTR_BUILD_URL = "https://nostr.build/api/v2/upload/files";
const MAX_FILE_SIZE = 10 * 1024 * 1024;

async function pickAndUpload(mediaType: "avatar" | "banner"): Promise<string> {
  const path = await open({
    multiple: false,
    filters: [
      { name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] },
    ],
  });
  if (!path) throw new Error("cancelled");

  const bytes = await readFile(path);
  if (bytes.length > MAX_FILE_SIZE) throw new Error("File must be under 10MB");

  const ext = path.split(".").pop()?.toLowerCase() ?? "png";
  const mime =
    ext === "jpg" || ext === "jpeg"
      ? "image/jpeg"
      : ext === "webp"
        ? "image/webp"
        : ext === "gif"
          ? "image/gif"
          : "image/png";

  const blob = new Blob([bytes], { type: mime });
  const file = new File([blob], `upload.${ext}`, { type: mime });

  const authHeader = await invoke<string>("create_nip98_auth", {
    url: NOSTR_BUILD_URL,
    method: "POST",
  });

  const formData = new FormData();
  formData.append("file", file);
  formData.append("media_type", mediaType);

  const res = await fetch(NOSTR_BUILD_URL, {
    method: "POST",
    headers: { Authorization: authHeader },
    body: formData,
  });

  if (!res.ok) {
    const text = await res.text().catch(() => "Unknown error");
    throw new Error(`Upload failed: ${res.status} - ${text}`);
  }

  const result = await res.json();
  if (result.status !== "success" || !result.data?.[0]?.url) {
    throw new Error("Upload failed — no URL returned");
  }
  return result.data[0].url;
}

export default function ProfilePage() {
  const profileOpen = useStore((s) => s.profileOpen);
  if (!profileOpen) return null;
  return <ProfilePageContent />;
}

function ProfilePageContent() {
  const nostrNpub = useStore((s) => s.nostrNpub);
  const nostrProfile = useStore((s) => s.nostrProfile);

  const [name, setName] = useState(nostrProfile?.name ?? "");
  const [displayName, setDisplayName] = useState(
    nostrProfile?.display_name ?? "",
  );
  const [picture, setPicture] = useState(nostrProfile?.picture ?? "");
  const [banner, setBanner] = useState(nostrProfile?.banner ?? "");
  const [about, setAbout] = useState(nostrProfile?.about ?? "");
  const [website, setWebsite] = useState(nostrProfile?.website ?? "");
  const [nip05, setNip05] = useState(nostrProfile?.nip05 ?? "");
  const [lud16, setLud16] = useState(nostrProfile?.lud16 ?? "");
  const [saving, setSaving] = useState(false);
  const [showMore, setShowMore] = useState(false);
  const [uploadingAvatar, setUploadingAvatar] = useState(false);
  const [uploadingBanner, setUploadingBanner] = useState(false);

  useEffect(() => {
    if (nostrProfile) {
      setName(nostrProfile.name ?? "");
      setDisplayName(nostrProfile.display_name ?? "");
      setPicture(nostrProfile.picture ?? "");
      setBanner(nostrProfile.banner ?? "");
      setAbout(nostrProfile.about ?? "");
      setWebsite(nostrProfile.website ?? "");
      setNip05(nostrProfile.nip05 ?? "");
      setLud16(nostrProfile.lud16 ?? "");
    }
  }, [nostrProfile]);

  const close = useCallback(() => {
    useStore.setState({ profileOpen: false });
  }, []);

  useLockScroll();
  useEscapeKey(true, close);

  useEffect(() => {
    invoke<import("../../types").NostrProfile | null>("fetch_nostr_profile")
      .then((profile) => {
        if (profile) useStore.setState({ nostrProfile: profile });
      })
      .catch(() => {});
  }, []);

  const handleBackdropClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (e.target === e.currentTarget) close();
    },
    [close],
  );

  const avatarSrc = picture || generateAvatarDataUri(nostrNpub || "default");

  const handleSave = useCallback(async () => {
    setSaving(true);
    try {
      await invoke("publish_nostr_profile", {
        name: name || displayName,
        picture: picture || null,
        displayName: displayName || null,
        about: about || null,
        website: website || null,
        nip05: nip05 || null,
        lud16: lud16 || null,
        banner: banner || null,
      });
      useStore.setState({
        nostrProfile: {
          name: name || undefined,
          display_name: displayName || undefined,
          picture: picture || undefined,
          banner: banner || undefined,
          about: about || undefined,
          website: website || undefined,
          nip05: nip05 || undefined,
          lud16: lud16 || undefined,
        },
      });
      showToast("Profile updated", "success");
      close();
    } catch (e) {
      showToast(`Failed to update profile: ${String(e)}`, "error");
    }
    setSaving(false);
  }, [name, displayName, picture, banner, about, website, nip05, lud16, close]);

  return (
    <div
      onClick={handleBackdropClick}
      className="macos-overlay-safe-top fixed inset-0 z-40 flex items-center justify-center bg-black/50 backdrop-blur-sm"
    >
      <div className="relative mx-4 flex max-h-[85vh] w-full max-w-2xl flex-col overflow-hidden rounded-2xl border border-slate-700 bg-slate-950 shadow-2xl">
        <div className="flex-1 overflow-y-auto">
          {/* Banner */}
          <div className="relative h-40 bg-slate-900">
            {/* Close + title overlay */}
            <div className="absolute inset-x-0 top-0 z-10 flex items-center justify-between px-4 py-3">
              <span className="text-lg font-medium text-white drop-shadow-md">
                Edit Profile
              </span>
              <CloseButton onClick={close} variant="overlay" />
            </div>
            {banner && (
              <img
                src={banner}
                alt="Banner"
                className="h-full w-full object-cover"
              />
            )}
            <button
              type="button"
              disabled={uploadingBanner}
              onClick={async () => {
                setUploadingBanner(true);
                try {
                  const url = await pickAndUpload("banner");
                  setBanner(url);
                } catch (e) {
                  if (String(e) !== "cancelled")
                    showToast(`Upload failed: ${String(e)}`, "error");
                }
                setUploadingBanner(false);
              }}
              className="absolute bottom-3 right-3 flex items-center gap-1.5 rounded-lg border border-slate-600 bg-slate-900/80 px-3 py-1.5 text-xs text-slate-300 backdrop-blur transition hover:bg-slate-800 disabled:opacity-50"
            >
              <svg
                aria-hidden="true"
                className="h-3.5 w-3.5"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth="2"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M3 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 0110.07 4h3.86a2 2 0 011.664.89l.812 1.22A2 2 0 0018.07 7H19a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V9z"
                />
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M15 13a3 3 0 11-6 0 3 3 0 016 0z"
                />
              </svg>
              {uploadingBanner ? "Uploading..." : "Change banner"}
            </button>
          </div>

          {/* Avatar overlaps banner; name/NIP-05/npub sit to its right,
              starting below the banner edge so nothing overlaps the image.
              items-start prevents the flex from stretching the avatar
              container (which would push its absolute-positioned camera
              button away from the avatar itself). */}
          <div className="px-6">
            <div className="-mt-8 mb-8 flex items-start gap-5">
              <div className="relative shrink-0">
                <img
                  src={avatarSrc}
                  alt="Avatar"
                  className="h-20 w-20 rounded-full border-4 border-slate-950 object-cover"
                />
                <button
                  type="button"
                  disabled={uploadingAvatar}
                  onClick={async () => {
                    setUploadingAvatar(true);
                    try {
                      const url = await pickAndUpload("avatar");
                      setPicture(url);
                    } catch (e) {
                      if (String(e) !== "cancelled")
                        showToast(`Upload failed: ${String(e)}`, "error");
                    }
                    setUploadingAvatar(false);
                  }}
                  className="absolute bottom-0 right-0 flex h-7 w-7 items-center justify-center rounded-full border border-slate-600 bg-slate-900 text-slate-400 transition hover:bg-slate-800 hover:text-slate-200 disabled:opacity-50"
                >
                  <svg
                    aria-hidden="true"
                    className="h-3.5 w-3.5"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    strokeWidth="2"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M3 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 0110.07 4h3.86a2 2 0 011.664.89l.812 1.22A2 2 0 0018.07 7H19a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V9z"
                    />
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M15 13a3 3 0 11-6 0 3 3 0 016 0z"
                    />
                  </svg>
                </button>
              </div>
              {/* mt-8 cancels the row's -mt-8 (back to banner edge), plus
                  an extra pt-3 for breathing room between banner and name. */}
              <div className="mt-8 min-w-0 flex-1 pt-3">
                <p className="text-lg font-medium text-slate-100 truncate">
                  {displayName || name || "Unnamed"}
                </p>
                {nip05 && (
                  <p className="mt-1.5 flex items-center gap-1.5 text-sm text-slate-300">
                    <svg
                      aria-hidden="true"
                      className="h-4 w-4 shrink-0"
                      viewBox="0 0 122.88 116.87"
                    >
                      <polygon
                        fill="#a855f7"
                        fillRule="evenodd"
                        points="61.37 8.24 80.43 0 90.88 17.79 111.15 22.32 109.15 42.85 122.88 58.43 109.2 73.87 111.15 94.55 91 99 80.43 116.87 61.51 108.62 42.45 116.87 32 99.08 11.73 94.55 13.73 74.01 0 58.43 13.68 42.99 11.73 22.32 31.88 17.87 42.45 0 61.37 8.24"
                      />
                      <path
                        fill="#fff"
                        d="M37.92,65c-6.07-6.53,3.25-16.26,10-10.1,2.38,2.17,5.84,5.34,8.24,7.49L74.66,39.66C81.1,33,91.27,42.78,84.91,49.48L61.67,77.2a7.13,7.13,0,0,1-9.9.44C47.83,73.89,42.05,68.5,37.92,65Z"
                      />
                    </svg>
                    {nip05}
                  </p>
                )}
                {nostrNpub && (
                  <button
                    type="button"
                    onClick={() => {
                      void navigator.clipboard.writeText(nostrNpub);
                      showToast("Copied npub to clipboard");
                    }}
                    className="group mt-0.5 flex items-center gap-1.5 mono text-xs text-slate-400 hover:text-white transition"
                    title="Click to copy"
                  >
                    {nostrNpub.slice(0, 16)}...{nostrNpub.slice(-6)}
                    <svg
                      aria-hidden="true"
                      className="h-3 w-3 shrink-0 text-slate-500 group-hover:text-slate-400"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    >
                      <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                    </svg>
                  </button>
                )}
              </div>
            </div>

            {/* Form */}
            <div className="space-y-5 pb-6">
              <Field
                label="Display Name"
                value={displayName}
                onChange={setDisplayName}
                placeholder="How you appear to others"
                maxLength={50}
              />
              <div>
                <label
                  htmlFor="profile-about"
                  className="mb-1.5 block text-xs font-medium uppercase tracking-wide text-slate-400"
                >
                  Bio
                </label>
                <textarea
                  id="profile-about"
                  value={about}
                  onChange={(e) => setAbout(e.target.value)}
                  placeholder="Tell people about yourself"
                  rows={3}
                  className="w-full rounded-lg border border-slate-700 bg-slate-900 px-4 py-3 text-sm text-slate-100 outline-none ring-emerald-400 transition focus:ring-2"
                />
              </div>
              <button
                type="button"
                onClick={() => setShowMore((v) => !v)}
                className="flex w-full items-center justify-between rounded-lg border border-slate-800 px-4 py-3 text-left transition hover:bg-slate-900/50"
              >
                <span className="text-xs font-medium text-slate-400">
                  More details
                </span>
                <svg
                  aria-hidden="true"
                  className={`h-4 w-4 text-slate-500 transition-transform ${showMore ? "rotate-180" : ""}`}
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth="2"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M19 9l-7 7-7-7"
                  />
                </svg>
              </button>
              {showMore && (
                <div className="space-y-5">
                  <Field
                    label="Username"
                    value={name}
                    onChange={setName}
                    placeholder="Unique handle (lowercase)"
                    maxLength={30}
                  />
                  <Field
                    label="Profile Picture URL"
                    value={picture}
                    onChange={setPicture}
                    placeholder="https://..."
                  />
                  <Field
                    label="Banner URL"
                    value={banner}
                    onChange={setBanner}
                    placeholder="https://..."
                  />
                  <Field
                    label="Website"
                    value={website}
                    onChange={setWebsite}
                    placeholder="https://..."
                  />
                  <Field
                    label="NIP-05 Identifier"
                    value={nip05}
                    onChange={setNip05}
                    placeholder="you@example.com"
                  />
                  <Field
                    label="Lightning Address"
                    value={lud16}
                    onChange={setLud16}
                    placeholder="you@getalby.com"
                  />
                </div>
              )}

              <button
                type="button"
                onClick={handleSave}
                disabled={saving}
                className="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 transition hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {saving ? (
                  <span className="inline-flex items-center">
                    Saving
                    <span className="ml-0.5 inline-flex">
                      <span className="loading-dot">.</span>
                      <span className="loading-dot">.</span>
                      <span className="loading-dot">.</span>
                    </span>
                  </span>
                ) : (
                  "Save Profile"
                )}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function Field({
  label,
  value,
  onChange,
  placeholder,
  maxLength,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  maxLength?: number;
}) {
  const id = `profile-${label.toLowerCase().replace(/\s+/g, "-")}`;
  return (
    <div>
      <label
        htmlFor={id}
        className="mb-1.5 block text-xs font-medium uppercase tracking-wide text-slate-400"
      >
        {label}
      </label>
      <input
        id={id}
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        maxLength={maxLength}
        className="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm text-slate-100 outline-none ring-emerald-400 transition focus:ring-2"
      />
    </div>
  );
}
