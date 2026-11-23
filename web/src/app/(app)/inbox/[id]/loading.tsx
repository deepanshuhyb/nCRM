import { TravelTitleSkeleton } from "@/components/travel";
import { LoadingPage, SkeletonRows } from "@/components/ui";

/** A conversation while it loads. The name you tapped is already in place. */
export default function Loading() {
  return (
    <LoadingPage>
      <div>
        <span className="skel mb-3" style={{ height: 16, width: 70 }} />
        <TravelTitleSkeleton kind="chat" />
        <span className="skel mt-3" style={{ height: 18, width: "min(320px, 70%)" }} />
      </div>
      <div className="panel p-4 sm:p-5">
        <SkeletonRows rows={4} />
      </div>
    </LoadingPage>
  );
}
