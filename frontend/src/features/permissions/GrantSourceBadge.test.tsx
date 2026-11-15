import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { GrantSourceBadge } from "./GrantSourceBadge";

describe("authorization source label", () => {
  it("permission_rows_expose_grant_source", () => {
    render(<div><GrantSourceBadge source="manual" /><GrantSourceBadge source="external_hr_role" /></div>);
    expect(screen.getByText("手動付与")).toBeInTheDocument();
    expect(screen.getByText("HR自動付与")).toBeInTheDocument();
  });
});
