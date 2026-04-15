import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { useStore } from "../../store";
import { generateAvatarDataUri } from "../../utils-react/avatar";
import { showToast } from "../shared/Toast";

export default function ProfilePage() {
  const nostrNpub = useStore((s) => s.nostrNpub);
  const nostrProfile = useStore((s) => s.nostrProfile);

  const [name, setName] = useState(nostrProfile?.name ?? "");
  const [displayName, setDisplayName] = useState(
    nostrProfile?.display_name ?? "",
  );
  const [picture, setPicture] = useState(nostrProfile?.picture ?? "");
  const [about, setAbout] = useState(nostrProfile?.about ?? "");
  const [website, setWebsite] = useState(nostrProfile?.website ?? "");
  const [nip05, setNip05] = useState(nostrProfile?.nip05 ?? "");
  const [lud16, setLud16] = useState(nostrProfile?.lud16 ?? "");
  const [saving, setSaving] = useState(false);

  // Sync from store when profile updates
  useEffect(() => {
    if (nostrProfile) {
      setName(nostrProfile.name ?? "");
      setDisplayName(nostrProfile.display_name ?? "");
      setPicture(nostrProfile.picture ?? "");
      setAbout(nostrProfile.about ?? "");
      setWebsite(nostrProfile.website ?? "");
      setNip05(nostrProfile.nip05 ?? "");
      setLud16(nostrProfile.lud16 ?? "");
    }
  }, [nostrProfile]);

  const avatarSrc =
    picture ||
    generateAvatarDataUri(displayName || name || nostrNpub || "default");

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
      });
      useStore.setState({
        nostrProfile: {
          name: name || undefined,
          display_name: displayName || undefined,
          picture: picture || undefined,
          about: about || undefined,
          website: website || undefined,
          nip05: nip05 || undefined,
          lud16: lud16 || undefined,
        },
      });
      showToast("Profile updated", "success");
    } catch (e) {
      showToast(`Failed to update profile: ${String(e)}`, "error");
    }
    setSaving(false);
  }, [name, displayName, picture, about, website, nip05, lud16]);

  const handleBack = useCallback(() => {
    useStore.setState({ view: "home" });
  }, []);

  return (
    <div className="phi-container py-6 lg:py-8">
      <div className="mx-auto max-w-lg">
        <button
          type="button"
          onClick={handleBack}
          className="mb-6 flex items-center gap-1.5 text-sm text-slate-400 transition hover:text-slate-200"
        >
          <svg
            aria-hidden="true"
            className="h-4 w-4"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth="2"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M15 19l-7-7 7-7"
            />
          </svg>
          Back
        </button>

        <h1 className="text-2xl font-semibold text-slate-100">Edit Profile</h1>
        <p className="mt-1 text-sm text-slate-400">
          Update your Nostr identity. Changes are published to your relays.
        </p>

        {/* Avatar */}
        <div className="mt-8 flex items-center gap-5">
          <img
            src={avatarSrc}
            alt="Avatar"
            className="h-20 w-20 rounded-full object-cover border border-slate-700"
          />
          <div className="min-w-0 flex-1">
            <p className="text-lg font-medium text-slate-100 truncate">
              {displayName || name || "Unnamed"}
            </p>
            {nostrNpub && (
              <p className="mt-0.5 mono text-xs text-slate-500 truncate">
                {nostrNpub}
              </p>
            )}
          </div>
        </div>

        {/* Form */}
        <div className="mt-8 space-y-5">
          <Field
            label="Display Name"
            value={displayName}
            onChange={setDisplayName}
            placeholder="How you appear to others"
            maxLength={50}
          />
          <Field
            label="Username"
            value={name}
            onChange={setName}
            placeholder="Unique handle (lowercase)"
            maxLength={30}
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
          <Field
            label="Profile Picture URL"
            value={picture}
            onChange={setPicture}
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

        <button
          type="button"
          onClick={handleSave}
          disabled={saving}
          className="mt-8 w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 transition hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed"
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
