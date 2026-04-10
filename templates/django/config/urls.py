import os

from django.http import JsonResponse
from django.urls import path


def index(_request):
    return JsonResponse(
        {
            "app": "django",
            "message": "Deku Django starter",
            "database_url_configured": bool(os.environ.get("DATABASE_URL")),
        }
    )


def health(_request):
    return JsonResponse({"status": "ok"})


urlpatterns = [
    path("", index),
    path("health", health),
]
