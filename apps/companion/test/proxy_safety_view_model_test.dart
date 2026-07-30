import 'dart:async';

import 'package:app_tester_companion/features/proxy_safety/data/proxy_safety_repository.dart';
import 'package:app_tester_companion/features/proxy_safety/presentation/proxy_safety_view_model.dart';
import 'package:flutter_test/flutter_test.dart';

class FakeRepository implements ProxySafetyRepository {
  ProxySafetyStatus current =
      const ProxySafetyStatus(isMonitoring: false, isVpnActive: false);
  String? armedHost;
  int? armedPort;

  @override
  Future<List<Map<String, String>>> installedDebugApps() async => const [];

  @override
  Future<ProxySafetyStatus> startMonitoring(
      {required String host, required int port}) async {
    armedHost = host;
    armedPort = port;
    return current = ProxySafetyStatus(
        isMonitoring: true, isVpnActive: false, host: host, port: port);
  }

  @override
  Future<ProxySafetyStatus> stopMonitoring() async =>
      current = const ProxySafetyStatus(isMonitoring: false, isVpnActive: false);

  @override
  Future<ProxySafetyStatus> startVpn({required String host, required int port, required String targetPackage}) async =>
      current = ProxySafetyStatus(isMonitoring: false, isVpnActive: true, host: host, port: port, targetPackage: targetPackage);

  @override
  Future<ProxySafetyStatus> stopVpn() async =>
      current = const ProxySafetyStatus(isMonitoring: false, isVpnActive: false);

  @override
  Future<ProxySafetyStatus> status() async => current;
}

void main() {
  test('starts monitoring the configured desktop endpoint', () async {
    final repository = FakeRepository();
    final model = ProxySafetyViewModel(repository);

    await model.startMonitoring('10.10.10.15', '8080');

    expect(repository.armedHost, '10.10.10.15');
    expect(repository.armedPort, 8080);
    expect(model.status?.isMonitoring, isTrue);
  });

  test('rejects an invalid proxy port before calling Android', () async {
    final repository = FakeRepository();
    final model = ProxySafetyViewModel(repository);

    await model.startMonitoring('10.10.10.15', '0');

    expect(repository.armedPort, isNull);
    expect(model.error, contains('1 to 65535'));
  });

  test('disconnects a paired desktop before VPN capture starts', () async {
    final repository = FakeRepository();
    final model = ProxySafetyViewModel(repository);
    await model.startMonitoring('10.10.10.15', '8080');

    await model.disconnect();

    expect(model.status?.isMonitoring, isFalse);
    expect(model.status?.isVpnActive, isFalse);
    expect(model.networkMatch, NetworkMatch.unknown);
  });

  test('shows a connecting state until QR registration completes', () async {
    final registration = Completer<void>();
    final model = ProxySafetyViewModel(FakeRepository(), registerCompanion: (_, __, ___, ____) => registration.future);

    final connecting = model.connectFromQr('{"protocol":"app-tester-companion","version":2,"host":"127.0.0.1","port":8080,"token":"token"}');
    await Future<void>.delayed(Duration.zero);

    expect(model.isWorking, isTrue);
    registration.complete();
    await connecting;
    expect(model.isWorking, isFalse);
    expect(model.status?.isMonitoring, isTrue);
  });
}
