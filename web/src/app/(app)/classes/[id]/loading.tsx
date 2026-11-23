import { TravelTitleSkeleton } from "@/components/travel";
import { LoadingPage, SkeletonRows } from "@/components/ui";

/** The class page while it loads. The name you tapped is already in place. */
export default function Loading() {
  return (
    <LoadingPage>
      <div>
        <span className="skel mb-3" style={{ height: 16, width: 110 }} />
        <TravelTitleSkeleton kind="class" />
        <span className="skel mt-3" style={{ height: 18, width: "min(420px, 80%)" }} />
      </div>
      <div className="panel p-4 sm:p-6">
        <SkeletonRows rows={5} />
      </div>
    </LoadingPage>
  );
}
