import { useCallback, useEffect, useRef } from "react";
import { useStore } from "../../store";
import type { MarketCategory } from "../../types";

const MARKET_CATEGORIES: MarketCategory[] = [
  "Bitcoin",
  "Politics",
  "Sports",
  "Culture",
  "Weather",
  "Macro",
];

export default function CategoryDropdown() {
  const category = useStore((s) => s.createCategory);
  const isOpen = useStore((s) => s.createCategoryOpen);
  const dropdownRef = useRef<HTMLDivElement>(null);

  const toggleOpen = useCallback(() => {
    useStore.setState((s) => ({
      createCategoryOpen: !s.createCategoryOpen,
    }));
  }, []);

  const selectCategory = useCallback((value: MarketCategory) => {
    useStore.setState({
      createCategory: value,
      createCategoryOpen: false,
    });
  }, []);

  // Close on outside click
  useEffect(() => {
    if (!isOpen) return;
    function handleClickOutside(e: MouseEvent) {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(e.target as Node)
      ) {
        useStore.setState({ createCategoryOpen: false });
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [isOpen]);

  return (
    <div className="relative" ref={dropdownRef}>
      <button
        type="button"
        onClick={toggleOpen}
        className="dc-input flex items-center justify-between gap-2 cursor-pointer text-left"
      >
        <span>{category}</span>
        <svg
          aria-hidden="true"
          xmlns="http://www.w3.org/2000/svg"
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={2}
          strokeLinecap="round"
          strokeLinejoin="round"
          className={`shrink-0 text-slate-400 transition-transform ${isOpen ? "rotate-180" : ""}`}
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>
      {isOpen && (
        <div className="dc-dropdown-menu right-0">
          {MARKET_CATEGORIES.map((item) => (
            <button
              key={item}
              type="button"
              onClick={() => selectCategory(item)}
              className={`dc-dropdown-option ${category === item ? "dc-dropdown-option-active" : ""}`}
            >
              {item}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
